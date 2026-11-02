//! The day's mood, chosen from recent activity by one ordered set of rules.

use crate::growth::BloomDevelopment;

/// How the flower moves for one day.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mood {
    /// Bent over and browning, dropping a petal.
    Wilt,
    /// Drooping a little, half closed, barely moving.
    Rest,
    /// Opening and closing to the sun, with glints around it.
    Unfold,
    /// Swaying hard and scattering pollen.
    Burst,
    /// Swaying in a light breeze; the everyday mood.
    Sway,
}

/// Every mood, for sheets and tests.
pub const MOODS: [Mood; 5] = [
    Mood::Sway,
    Mood::Burst,
    Mood::Unfold,
    Mood::Rest,
    Mood::Wilt,
];

impl Mood {
    /// A lowercase name for captions.
    pub fn name(self) -> &'static str {
        match self {
            Self::Wilt => "wilting",
            Self::Rest => "resting",
            Self::Unfold => "unfolding",
            Self::Burst => "bursting",
            Self::Sway => "swaying",
        }
    }
}

const WILT_QUIET_DAYS: u32 = 60;
const REST_QUIET_DAYS: u32 = 14;
/// Totals whose crossing earns a week of bursting.
const BURST_MILESTONES: [u64; 7] = [40, 100, 500, 1_000, 2_500, 5_000, 10_000];
const BUSY_COMPARED_WEEKS: usize = 52;
const BUSY_MIN_COMPLETE_WEEKS: usize = 10;
const BUSY_WEEK_SHARE_DENOMINATOR: usize = 10;

/// Choose the mood for the day after the observation date. The first matching rule wins:
///
/// | Mood | When the observation date |
/// | --- | --- |
/// | Wilt | ends 60 or more days without commits |
/// | Rest | ends 14 or more days without commits |
/// | Unfold | is the first day with commits in its Monday-Sunday week |
/// | Burst | falls within seven days of the total crossing a milestone |
/// | Burst | is a Sunday ending a week in the top tenth of the last 52 |
/// | Sway | is anything else |
pub fn select_mood(development: &BloomDevelopment) -> Mood {
    if development.quiet_day_count >= WILT_QUIET_DAYS {
        Mood::Wilt
    } else if development.quiet_day_count >= REST_QUIET_DAYS {
        Mood::Rest
    } else if is_first_commit_of_week(development) {
        Mood::Unfold
    } else if has_crossed_milestone(development) || is_finished_busy_week(development) {
        Mood::Burst
    } else {
        Mood::Sway
    }
}

fn is_first_commit_of_week(development: &BloomDevelopment) -> bool {
    matches!(
        development.observation_week_commits.split_last(),
        Some((&observed_day, earlier_days))
            if observed_day > 0 && earlier_days.iter().all(|&commits| commits == 0)
    )
}

fn has_crossed_milestone(development: &BloomDevelopment) -> bool {
    let total = development.total_commits;
    let earlier_total = total.saturating_sub(development.recent_commits);
    BURST_MILESTONES
        .iter()
        .any(|&milestone| earlier_total < milestone && milestone <= total)
}

fn is_finished_busy_week(development: &BloomDevelopment) -> bool {
    let finished_week = &development.observation_week_commits;
    let complete_weeks = &development.complete_week_commits;
    let compared_weeks =
        &complete_weeks[complete_weeks.len().saturating_sub(BUSY_COMPARED_WEEKS)..];
    let week_total: u64 = finished_week.iter().sum();
    let matching_weeks = compared_weeks
        .iter()
        .filter(|&&week_commits| week_commits >= week_total)
        .count();
    finished_week.len() == 7
        && week_total > 0
        && compared_weeks.len() >= BUSY_MIN_COMPLETE_WEEKS
        && matching_weeks * BUSY_WEEK_SHARE_DENOMINATOR <= compared_weeks.len()
}
