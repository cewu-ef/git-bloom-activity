//! Collection: the only stage that runs programs. It reads commit dates with `git` and
//! the repository's languages with the GitHub CLI.

use std::path::Path;
use std::process::Command;

use chrono::NaiveDate;

use crate::activity::{RepositoryActivity, count_daily_commits, parse_languages};

/// The GraphQL query for the repository's ID and languages, largest first.
pub const LANGUAGE_QUERY: &str = "query($owner: String!, $name: String!) { repository(owner: $owner, name: $name) { databaseId languages(first: 100, orderBy: {field: SIZE, direction: DESC}) { edges { size node { name color } } } } }";

/// Read the repository checked out at `repository_directory` as of `observed_on`.
///
/// Commits come from the checked-out branch, merges excluded, dated by author in UTC.
/// A shallow clone fails, since it would hide history; check out with `fetch-depth: 0`.
pub fn collect_activity(
    repository_directory: &Path,
    repository: &str,
    observed_on: NaiveDate,
) -> Result<RepositoryActivity, String> {
    let (owner, name) = repository
        .split_once('/')
        .filter(|(owner, name)| !owner.is_empty() && !name.is_empty() && !name.contains('/'))
        .ok_or_else(|| format!("{repository} is not OWNER/NAME"))?;
    let shallow = run_git(
        repository_directory,
        &["rev-parse", "--is-shallow-repository"],
    )?;
    if shallow.trim() == "true" {
        return Err(
            "the checkout is shallow, so most history is missing; set `fetch-depth: 0` on actions/checkout"
                .into(),
        );
    }
    let has_commits = Command::new("git")
        .current_dir(repository_directory)
        .args(["rev-parse", "--verify", "--quiet", "HEAD"])
        .output()
        .map_err(|spawn_error| format!("could not run git: {spawn_error}"))?
        .status
        .success();
    let log_text = if has_commits {
        run_git(
            repository_directory,
            &[
                "log",
                "--no-merges",
                "--date=format-local:%Y-%m-%d",
                "--format=%ad%x09%ae",
                "HEAD",
            ],
        )?
    } else {
        String::new()
    };
    let daily_commits =
        count_daily_commits(&log_text, observed_on).map_err(|error| error.to_string())?;
    let response_text = run(
        Command::new("gh").args([
            "api",
            "graphql",
            "-f",
            &format!("query={LANGUAGE_QUERY}"),
            "-f",
            &format!("owner={owner}"),
            "-f",
            &format!("name={name}"),
        ]),
        "gh",
    )?;
    let (identity_seed, languages) =
        parse_languages(&response_text).map_err(|error| error.to_string())?;
    Ok(RepositoryActivity {
        repository: repository.to_owned(),
        identity_seed,
        languages,
        daily_commits,
        observed_on,
    })
}

/// `owner/name` of the repository: `GITHUB_REPOSITORY` in Actions, else what `gh`
/// says the checkout belongs to.
pub fn find_repository_name(repository_directory: &Path) -> Result<String, String> {
    if let Ok(repository) = std::env::var("GITHUB_REPOSITORY")
        && !repository.is_empty()
    {
        return Ok(repository);
    }
    let name = run(
        Command::new("gh").current_dir(repository_directory).args([
            "repo",
            "view",
            "--json",
            "nameWithOwner",
            "--jq",
            ".nameWithOwner",
        ]),
        "gh",
    )?;
    Ok(name.trim().to_owned())
}

fn run_git(repository_directory: &Path, arguments: &[&str]) -> Result<String, String> {
    run(
        Command::new("git")
            .current_dir(repository_directory)
            .env("TZ", "UTC")
            .args(arguments),
        "git",
    )
}

fn run(command: &mut Command, program: &str) -> Result<String, String> {
    let output = command
        .output()
        .map_err(|spawn_error| format!("could not run {program}: {spawn_error}"))?;
    if !output.status.success() {
        return Err(format!(
            "{program} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    String::from_utf8(output.stdout)
        .map_err(|_| format!("{program} printed text that is not UTF-8"))
}
