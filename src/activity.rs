//! What one repository's history says, read from git and GitHub, before any judgment.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

use chrono::NaiveDate;
use serde_json::Value;

/// One language GitHub detected in the repository.
#[derive(Clone, Debug, PartialEq)]
pub struct Language {
    /// GitHub's name for it, such as `Rust`.
    pub name: String,
    /// GitHub's color for it; `None` means GitHub gives it no color.
    pub color: Option<[u8; 3]>,
    /// Bytes of code in this language.
    pub bytes: u64,
}

/// Observations of one repository as of one day.
#[derive(Clone, Debug, PartialEq)]
pub struct RepositoryActivity {
    /// `owner/name`.
    pub repository: String,
    /// A stable number for the repository, so its flower keeps its look.
    pub identity_seed: u64,
    /// Languages by size, largest first.
    pub languages: Vec<Language>,
    /// Commits per UTC day, every day from the first commit through `observed_on`.
    /// A repository without commits has one entry, for `observed_on`, with zero.
    pub daily_commits: BTreeMap<NaiveDate, u32>,
    /// The last complete day the observation covers.
    pub observed_on: NaiveDate,
}

/// Why history could not be read. The message names the repair.
#[derive(Debug, Eq, PartialEq)]
pub struct ActivityError(pub String);

impl fmt::Display for ActivityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for ActivityError {}

/// Count commits per day from `git log` lines of `YYYY-MM-DD<TAB>author-email`.
///
/// Commits by GitHub Apps (authors ending in `[bot]@users.noreply.github.com`) are left
/// out, so the workflow's own commits and Dependabot never make a repository look busy.
/// Days after `observed_on` are ignored. Every day from the first commit through
/// `observed_on` is present, with zero for quiet days.
pub fn count_daily_commits(
    log_text: &str,
    observed_on: NaiveDate,
) -> Result<BTreeMap<NaiveDate, u32>, ActivityError> {
    let mut daily_commits = BTreeMap::new();
    for log_line in log_text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let (date_text, author_email) = log_line.split_once('\t').unwrap_or((log_line, ""));
        if is_bot_email(author_email) {
            continue;
        }
        let commit_date = NaiveDate::parse_from_str(date_text, "%Y-%m-%d").map_err(|_| {
            ActivityError(format!(
                "git log printed a date that is not YYYY-MM-DD: {log_line}"
            ))
        })?;
        if commit_date <= observed_on {
            *daily_commits.entry(commit_date).or_insert(0) += 1;
        }
    }
    let first_day = daily_commits.keys().next().copied().unwrap_or(observed_on);
    for day in first_day.iter_days().take_while(|day| *day <= observed_on) {
        daily_commits.entry(day).or_insert(0);
    }
    Ok(daily_commits)
}

fn is_bot_email(author_email: &str) -> bool {
    author_email.ends_with("[bot]@users.noreply.github.com")
}

/// Read the repository's database ID and languages from a GraphQL response to
/// [`crate::collect::LANGUAGE_QUERY`].
pub fn parse_languages(response_text: &str) -> Result<(u64, Vec<Language>), ActivityError> {
    let response: Value = serde_json::from_str(response_text).map_err(|parse_error| {
        ActivityError(format!("GitHub's answer is not JSON: {parse_error}"))
    })?;
    if let Some(message) = response
        .pointer("/errors/0/message")
        .and_then(Value::as_str)
    {
        return Err(ActivityError(format!(
            "GitHub refused the language query: {message}"
        )));
    }
    let repository = response
        .pointer("/data/repository")
        .filter(|repository| !repository.is_null())
        .ok_or_else(|| {
            ActivityError(
                "GitHub found no such repository; check the name and the token's access".into(),
            )
        })?;
    let identity_seed = repository["databaseId"]
        .as_u64()
        .ok_or_else(|| ActivityError("GitHub sent no repository ID".into()))?;
    let mut languages = Vec::new();
    for edge in repository
        .pointer("/languages/edges")
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice)
    {
        let name = edge
            .pointer("/node/name")
            .and_then(Value::as_str)
            .ok_or_else(|| ActivityError("GitHub sent a language without a name".into()))?;
        let bytes = edge["size"]
            .as_u64()
            .ok_or_else(|| ActivityError(format!("GitHub sent no size for {name}")))?;
        let color = match edge.pointer("/node/color").and_then(Value::as_str) {
            None => None,
            Some(color_text) => Some(parse_hex_color(color_text).ok_or_else(|| {
                ActivityError(format!(
                    "GitHub sent {name} an unreadable color {color_text}"
                ))
            })?),
        };
        languages.push(Language {
            name: name.to_owned(),
            color,
            bytes,
        });
    }
    languages.sort_by(|left, right| {
        right
            .bytes
            .cmp(&left.bytes)
            .then(left.name.cmp(&right.name))
    });
    Ok((identity_seed, languages))
}

/// Read `#rrggbb`.
pub fn parse_hex_color(color_text: &str) -> Option<[u8; 3]> {
    let hex_digits = color_text.strip_prefix('#')?;
    if hex_digits.len() != 6 || !hex_digits.is_ascii() {
        return None;
    }
    let channel = |index: usize| u8::from_str_radix(&hex_digits[index..index + 2], 16).ok();
    Some([channel(0)?, channel(2)?, channel(4)?])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(text: &str) -> NaiveDate {
        NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn counts_fill_quiet_days_and_skip_bots_and_the_future() {
        let log_text = "2026-01-05\tdev@example.com\n\
                        2026-01-03\tdev@example.com\n\
                        2026-01-03\tother@example.com\n\
                        2026-01-04\t41898282+github-actions[bot]@users.noreply.github.com\n\
                        2026-01-09\tdev@example.com\n";
        let daily_commits = count_daily_commits(log_text, date("2026-01-06")).unwrap();
        let counts: Vec<u32> = daily_commits.values().copied().collect();
        assert_eq!(daily_commits.keys().next(), Some(&date("2026-01-03")));
        assert_eq!(counts, [2, 0, 1, 0]);
    }

    #[test]
    fn an_empty_history_is_one_quiet_day() {
        let daily_commits = count_daily_commits("", date("2026-01-06")).unwrap();
        assert_eq!(daily_commits.len(), 1);
        assert_eq!(daily_commits[&date("2026-01-06")], 0);
    }

    #[test]
    fn languages_come_largest_first_with_colors() {
        let response = r##"{"data":{"repository":{"databaseId":42,"languages":{"edges":[
            {"size":10,"node":{"name":"Shell","color":"#89e051"}},
            {"size":900,"node":{"name":"Rust","color":"#dea584"}},
            {"size":5,"node":{"name":"Nix","color":null}}]}}}}"##;
        let (identity_seed, languages) = parse_languages(response).unwrap();
        assert_eq!(identity_seed, 42);
        assert_eq!(languages[0].name, "Rust");
        assert_eq!(languages[0].color, Some([0xde, 0xa5, 0x84]));
        assert_eq!(languages[2].color, None);
    }

    #[test]
    fn a_missing_repository_names_the_repair() {
        let error = parse_languages(r#"{"data":{"repository":null}}"#).unwrap_err();
        assert!(error.0.contains("check the name"));
    }
}
