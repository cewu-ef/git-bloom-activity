use std::collections::BTreeMap;

use chrono::{Days, NaiveDate};
use git_bloom_activity::activity::{Language, RepositoryActivity};
use git_bloom_activity::growth::{Stage, derive_development, growth_fraction};
use git_bloom_activity::mood::{Mood, select_mood};

/// 2026-11-01 is a Sunday.
fn sunday() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, 1).unwrap()
}

/// Activity whose last day is `observed_on`, with `counts` ending on that day.
fn activity(counts: &[u32], observed_on: NaiveDate) -> RepositoryActivity {
    let first_day = observed_on
        .checked_sub_days(Days::new(counts.len() as u64 - 1))
        .unwrap();
    let daily_commits: BTreeMap<NaiveDate, u32> =
        first_day.iter_days().zip(counts.iter().copied()).collect();
    RepositoryActivity {
        repository: "example/bloom".into(),
        identity_seed: 1,
        languages: vec![
            Language {
                name: "Rust".into(),
                color: Some([222, 165, 132]),
                bytes: 600,
            },
            Language {
                name: "Shell".into(),
                color: Some([137, 224, 81]),
                bytes: 200,
            },
            Language {
                name: "Nix".into(),
                color: None,
                bytes: 100,
            },
            Language {
                name: "Just".into(),
                color: None,
                bytes: 60,
            },
            Language {
                name: "Make".into(),
                color: None,
                bytes: 40,
            },
        ],
        daily_commits,
        observed_on,
    }
}

/// `weeks` of one commit each Monday, ending with the week of `observed_on`.
fn weekly_counts(weeks: usize, then: &[u32]) -> Vec<u32> {
    let mut counts: Vec<u32> = (0..weeks).flat_map(|_| [1, 0, 0, 0, 0, 0, 0]).collect();
    counts.extend_from_slice(then);
    counts
}

#[test]
fn stages_follow_the_commit_total() {
    assert_eq!(Stage::of(0), Stage::Seed);
    assert_eq!(Stage::of(1), Stage::Sprout);
    assert_eq!(Stage::of(10), Stage::Bud);
    assert_eq!(Stage::of(40), Stage::Bloom);
    assert_eq!(growth_fraction(0), 0.0);
    assert_eq!(growth_fraction(10_000), 1.0);
    assert_eq!(growth_fraction(50_000), 1.0);
    assert!(growth_fraction(100) < growth_fraction(101));
}

#[test]
fn small_languages_join_the_largest_and_shares_sum_to_one() {
    let development = derive_development(&activity(&[1], sunday()));
    assert_eq!(development.languages.len(), 4);
    assert_eq!(development.languages[0].name, "Rust");
    assert!((development.languages[0].share - 0.64).abs() < 1e-9);
    let total: f64 = development
        .languages
        .iter()
        .map(|language| language.share)
        .sum();
    assert!((total - 1.0).abs() < 1e-9);
}

#[test]
fn a_quiet_observation_week_does_not_end_the_streak() {
    // Five active weeks, then a Monday and Tuesday without commits.
    let mut counts = weekly_counts(5, &[]);
    counts.extend([0, 0]);
    let tuesday = sunday().checked_add_days(Days::new(2)).unwrap();
    let development = derive_development(&activity(&counts, tuesday));
    assert_eq!(development.current_streak_weeks, 5);
    assert_eq!(development.longest_streak_weeks, 5);
}

#[test]
fn a_whole_quiet_week_ends_the_streak_but_keeps_the_record() {
    let mut counts = weekly_counts(6, &[]);
    counts.extend([0; 7]);
    counts.extend(weekly_counts(2, &[]));
    let development = derive_development(&activity(&counts, sunday()));
    assert_eq!(development.current_streak_weeks, 2);
    assert_eq!(development.longest_streak_weeks, 6);
}

#[test]
fn long_quiet_spells_rest_then_wilt() {
    let mut counts = vec![3];
    counts.extend([0; 20]);
    assert_eq!(
        select_mood(&derive_development(&activity(&counts, sunday()))),
        Mood::Rest
    );
    counts.extend([0; 50]);
    assert_eq!(
        select_mood(&derive_development(&activity(&counts, sunday()))),
        Mood::Wilt
    );
}

#[test]
fn the_first_commit_of_a_week_unfolds() {
    // Ten quiet days, then a commit on Wednesday.
    let wednesday = sunday().checked_add_days(Days::new(3)).unwrap();
    let mut counts = vec![50];
    counts.extend([0; 9]);
    counts.extend([0, 0, 2]);
    assert_eq!(
        select_mood(&derive_development(&activity(&counts, wednesday))),
        Mood::Unfold
    );
}

#[test]
fn crossing_a_milestone_bursts_and_steady_work_sways() {
    let mut counts = vec![0; 30];
    counts[0] = 95;
    counts[29] = 1;
    counts[27] = 5;
    let crossed = derive_development(&activity(&counts, sunday()));
    assert_eq!(crossed.total_commits, 101);
    assert_eq!(select_mood(&crossed), Mood::Burst);

    let steady = weekly_counts(20, &[1, 1, 0, 0, 0, 0, 0]);
    let saturday = sunday().checked_sub_days(Days::new(1)).unwrap();
    let steady_development = derive_development(&activity(&steady[..steady.len() - 1], saturday));
    assert_eq!(select_mood(&steady_development), Mood::Sway);
}

#[test]
fn a_sunday_ending_a_top_week_bursts() {
    let busy = weekly_counts(20, &[3, 3, 3, 3, 3, 3, 3]);
    let development = derive_development(&activity(&busy[1..], sunday()));
    assert_eq!(select_mood(&development), Mood::Burst);
}
