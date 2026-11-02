//! Reproducible development from a repository's activity, independent of drawing.

use std::collections::BTreeMap;

use chrono::{Datelike, Days, IsoWeek, NaiveDate};

use crate::activity::RepositoryActivity;

/// The first commit plants a sprout.
pub const SPROUT_COMMITS: u64 = 1;
/// A bud forms at this many commits.
pub const BUD_COMMITS: u64 = 10;
/// The flower opens at this many commits.
pub const BLOOM_COMMITS: u64 = 40;
/// Petals show pale until this total.
pub const FULL_COLOR_COMMITS: u64 = 250;
/// Growth stops here; larger totals keep the largest flower.
pub const MAX_GROWTH_COMMITS: u64 = 10_000;
/// At most this many languages get their own petals; smaller ones join the largest.
pub const PETAL_LANGUAGE_LIMIT: usize = 4;

/// How far along the plant is.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Stage {
    /// No commits yet: a seed in the soil.
    Seed,
    /// A stem with two seed leaves.
    Sprout,
    /// A closed bud showing the first color.
    Bud,
    /// An open flower.
    Bloom,
}

impl Stage {
    /// The stage a commit total reaches.
    pub fn of(total_commits: u64) -> Self {
        match total_commits {
            total if total >= BLOOM_COMMITS => Self::Bloom,
            total if total >= BUD_COMMITS => Self::Bud,
            total if total >= SPROUT_COMMITS => Self::Sprout,
            _ => Self::Seed,
        }
    }

    /// A lowercase name for captions.
    pub fn name(self) -> &'static str {
        match self {
            Self::Seed => "seed",
            Self::Sprout => "sprout",
            Self::Bud => "bud",
            Self::Bloom => "bloom",
        }
    }
}

/// Map a commit total to a growth fraction from zero through one. Gains diminish
/// until [`MAX_GROWTH_COMMITS`]; larger totals stay at one.
pub fn growth_fraction(total_commits: u64) -> f64 {
    const COMMIT_SCALE: f64 = 10.0;
    let bounded_total = total_commits.min(MAX_GROWTH_COMMITS) as f64;
    (1.0 + bounded_total / COMMIT_SCALE).log2()
        / (1.0 + MAX_GROWTH_COMMITS as f64 / COMMIT_SCALE).log2()
}

/// One language as the petals show it.
#[derive(Clone, Debug, PartialEq)]
pub struct PetalLanguage {
    /// GitHub's name for it.
    pub name: String,
    /// GitHub's color for it, if any.
    pub color: Option<[u8; 3]>,
    /// Its share of the repository's code, from zero to one.
    pub share: f64,
}

/// Everything the drawing reads, derived from one observation.
#[derive(Clone, Debug, PartialEq)]
pub struct BloomDevelopment {
    /// `owner/name`.
    pub repository: String,
    /// Places the petals and picks small variations, so the flower keeps its look.
    pub identity_seed: u64,
    /// Up to [`PETAL_LANGUAGE_LIMIT`] languages, largest share first; shares sum to one.
    pub languages: Vec<PetalLanguage>,
    /// Commits through the observation date, bots left out.
    pub total_commits: u64,
    /// The day every measurement is taken against.
    pub observed_on: NaiveDate,
    /// Monday-Sunday weeks with a commit in the streak still running. The observation
    /// week is in progress, so a quiet one does not end the streak yet.
    pub current_streak_weeks: u32,
    /// The longest run of active weeks ever.
    pub longest_streak_weeks: u32,
    /// Days in the commit-free run ending on the observation date.
    pub quiet_day_count: u32,
    /// Commits in the seven days ending on the observation date.
    pub recent_commits: u64,
    /// Commits in each complete Monday-Sunday week before the observation week, oldest first.
    pub complete_week_commits: Vec<u64>,
    /// Commits on each day of the observation week, Monday through the observation date.
    pub observation_week_commits: Vec<u64>,
}

impl BloomDevelopment {
    /// The stage the commit total has reached.
    pub fn stage(&self) -> Stage {
        Stage::of(self.total_commits)
    }
}

/// Derive what the flower shows from a repository's activity. Reads no clock.
pub fn derive_development(activity: &RepositoryActivity) -> BloomDevelopment {
    let daily_commits = &activity.daily_commits;
    let observed_on = activity.observed_on;
    let total_commits = daily_commits.values().map(|&count| u64::from(count)).sum();

    let mut weekly: BTreeMap<IsoWeek, (u64, u32)> = BTreeMap::new();
    for (day, &count) in daily_commits {
        let (week_commits, week_days) = weekly.entry(day.iso_week()).or_default();
        *week_commits += u64::from(count);
        *week_days += 1;
    }
    let observation_week = observed_on.iso_week();
    let active_weeks: Vec<bool> = weekly.values().map(|&(commits, _)| commits > 0).collect();
    let mut longest_streak_weeks = 0;
    let mut running_weeks = 0;
    for &active in &active_weeks {
        running_weeks = if active { running_weeks + 1 } else { 0 };
        longest_streak_weeks = longest_streak_weeks.max(running_weeks);
    }
    let counted_weeks = match active_weeks.split_last() {
        Some((false, earlier_weeks)) => earlier_weeks,
        _ => &active_weeks[..],
    };
    let current_streak_weeks = counted_weeks
        .iter()
        .rev()
        .take_while(|&&active| active)
        .count();

    let quiet_day_count = daily_commits
        .values()
        .rev()
        .take_while(|&&count| count == 0)
        .count();
    let recent_start = observed_on
        .checked_sub_days(Days::new(6))
        .expect("observation dates are far from the calendar's start");
    let recent_commits = daily_commits
        .range(recent_start..=observed_on)
        .map(|(_, &count)| u64::from(count))
        .sum();
    let observation_monday = observed_on
        .checked_sub_days(Days::new(u64::from(
            observed_on.weekday().num_days_from_monday(),
        )))
        .expect("observation dates are far from the calendar's start");

    BloomDevelopment {
        repository: activity.repository.clone(),
        identity_seed: activity.identity_seed,
        languages: share_languages(activity),
        total_commits,
        observed_on,
        current_streak_weeks: current_streak_weeks as u32,
        longest_streak_weeks,
        quiet_day_count: quiet_day_count as u32,
        recent_commits,
        complete_week_commits: weekly
            .iter()
            .filter(|&(week, &(_, days))| days == 7 && *week != observation_week)
            .map(|(_, &(commits, _))| commits)
            .collect(),
        observation_week_commits: daily_commits
            .range(observation_monday..=observed_on)
            .map(|(_, &count)| u64::from(count))
            .collect(),
    }
}

fn share_languages(activity: &RepositoryActivity) -> Vec<PetalLanguage> {
    let total_bytes: u64 = activity
        .languages
        .iter()
        .map(|language| language.bytes)
        .sum();
    if total_bytes == 0 {
        return Vec::new();
    }
    let mut petal_languages: Vec<PetalLanguage> = activity
        .languages
        .iter()
        .take(PETAL_LANGUAGE_LIMIT)
        .map(|language| PetalLanguage {
            name: language.name.clone(),
            color: language.color,
            share: language.bytes as f64 / total_bytes as f64,
        })
        .collect();
    let remaining_share: f64 = activity
        .languages
        .iter()
        .skip(PETAL_LANGUAGE_LIMIT)
        .map(|language| language.bytes as f64 / total_bytes as f64)
        .sum();
    petal_languages[0].share += remaining_share;
    petal_languages
}
