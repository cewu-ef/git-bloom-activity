use chrono::NaiveDate;
use git_bloom_activity::activity::parse_hex_color;
use git_bloom_activity::growth::{BloomDevelopment, PetalLanguage};
use git_bloom_activity::mood::MOODS;
use git_bloom_activity::output::encode_gif;
use git_bloom_activity::render::{
    BloomFrame, CANVAS_PIXELS, flower_count, frame_count, render_frame,
};

fn development(total_commits: u64, seed: u64, languages: &[(&str, &str, f64)]) -> BloomDevelopment {
    BloomDevelopment {
        repository: "example/bloom".into(),
        identity_seed: seed,
        languages: languages
            .iter()
            .map(|&(name, color, share)| PetalLanguage {
                name: name.into(),
                color: parse_hex_color(color),
                share,
            })
            .collect(),
        total_commits,
        observed_on: NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(),
        current_streak_weeks: 12,
        longest_streak_weeks: 90,
        quiet_day_count: 0,
        recent_commits: 0,
        complete_week_commits: Vec::new(),
        observation_week_commits: Vec::new(),
    }
}

const FOUR: &[(&str, &str, f64)] = &[
    ("TypeScript", "#3178c6", 0.4),
    ("Rust", "#dea584", 0.3),
    ("C", "#555555", 0.2),
    ("Shell", "#89e051", 0.1),
];

fn border_is_clear(frame: &BloomFrame) -> bool {
    (0..CANVAS_PIXELS).all(|edge| {
        [
            (edge, 0),
            (0, edge),
            (edge, CANVAS_PIXELS - 1),
            (CANVAS_PIXELS - 1, edge),
        ]
        .iter()
        .all(|&(x, y)| frame.pixel(x, y)[3] == 0)
    })
}

#[test]
fn identical_inputs_draw_identical_pixels() {
    let bloom = development(1_000, 3, FOUR);
    assert_eq!(
        render_frame(&bloom, MOODS[0], 7),
        render_frame(&bloom, MOODS[0], 7)
    );
    assert_eq!(
        render_frame(&bloom, MOODS[0], 0),
        render_frame(&bloom, MOODS[0], 120)
    );
}

#[test]
fn growth_changes_the_drawing() {
    let small = render_frame(&development(60, 3, FOUR), MOODS[0], 0);
    let large = render_frame(&development(6_000, 3, FOUR), MOODS[0], 0);
    let drawn = |frame: &BloomFrame| {
        frame
            .as_rgba_bytes()
            .chunks(4)
            .filter(|pixel| pixel[3] != 0)
            .count()
    };
    assert!(drawn(&large) > drawn(&small) * 2);
}

#[test]
fn languages_and_seeds_change_the_drawing() {
    let base = render_frame(&development(1_000, 3, FOUR), MOODS[0], 0);
    let other_seed = render_frame(&development(1_000, 4, FOUR), MOODS[0], 0);
    let one_language = render_frame(
        &development(1_000, 3, &[("Go", "#00ADD8", 1.0)]),
        MOODS[0],
        0,
    );
    assert_ne!(base, other_seed);
    assert_ne!(base, one_language);
}

#[test]
fn every_stage_and_mood_stays_inside_the_canvas_and_encodes() {
    for total_commits in [0, 3, 20, 45, 500, 10_000] {
        for seed in [1, 2] {
            let bloom = development(total_commits, seed, FOUR);
            for mood in MOODS {
                let frames: Vec<BloomFrame> = (0..frame_count(mood))
                    .map(|index| render_frame(&bloom, mood, index))
                    .collect();
                for (index, frame) in frames.iter().enumerate() {
                    assert!(
                        border_is_clear(frame),
                        "{total_commits} commits, seed {seed}, {mood:?} frame {index} touches the edge"
                    );
                }
                encode_gif(&frames)
                    .unwrap_or_else(|error| panic!("{total_commits} commits, {mood:?}: {error}"));
            }
        }
    }
}

#[test]
fn more_commits_grow_more_flowers() {
    let counts: Vec<usize> = [40, 150, 600, 2_000, 6_000, 50_000]
        .into_iter()
        .map(flower_count)
        .collect();
    assert_eq!(counts, [1, 2, 3, 4, 5, 5]);
}

#[test]
fn pink_leads_and_languages_stay_visible() {
    let frame = render_frame(
        &development(3_000, 5, &[("Go", "#00ADD8", 1.0)]),
        MOODS[0],
        0,
    );
    let pixels: Vec<&[u8]> = frame
        .as_rgba_bytes()
        .chunks(4)
        .filter(|pixel| pixel[3] != 0)
        .collect();
    let pink = pixels
        .iter()
        .filter(|pixel| pixel[0] > 150 && pixel[0] as u32 > pixel[1] as u32 + 50 && pixel[2] > 80)
        .count();
    let cyan = pixels
        .iter()
        .filter(|pixel| pixel[2] > 150 && pixel[0] < 100)
        .count();
    assert!(
        pink > cyan,
        "{pink} pink pixels, {cyan} language-colored ones"
    );
    assert!(cyan > 150, "only {cyan} language-colored pixels");
}
