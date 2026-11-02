use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{env, fs};

use chrono::{DateTime, NaiveDate};
use git_bloom_activity::collect::{collect_activity, find_repository_name};
use git_bloom_activity::growth::{BloomDevelopment, PetalLanguage, derive_development};
use git_bloom_activity::mood::{MOODS, Mood, select_mood};
use git_bloom_activity::output::{encode_gif, encode_png, encode_preview_html, encode_sheet_png};
use git_bloom_activity::render::{BloomFrame, frame_count, render_frame};

const USAGE: &str = "Usage: git-bloom-activity render --output-dir DIR [--repo-dir DIR]
                          [--repository OWNER/NAME] [--observed-on YYYY-MM-DD]
       git-bloom-activity sheet --output FILE
       git-bloom-activity --help | --version
render reads the repository checked out in --repo-dir (default: the current
directory) and writes bloom.gif, bloom.png, and preview.html into DIR. Commits come
from git; languages come from GitHub through an authenticated `gh` (in Actions, set
GH_TOKEN). The repository name defaults to GITHUB_REPOSITORY, else what `gh` reports.
The observation date defaults to yesterday in UTC, the last complete day.
sheet writes a PNG of every stage and mood, drawn from made-up activity.
Errors go to stderr and exit 1; invalid usage exits 2.";

fn main() -> ExitCode {
    let arguments: Vec<String> = env::args().skip(1).collect();
    let words: Vec<&str> = arguments.iter().map(String::as_str).collect();
    let result = match words.as_slice() {
        ["--help"] | [_, "--help"] => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        ["--version"] => {
            println!("git-bloom-activity {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        ["render", options @ ..] => match parse_render_options(options) {
            Some(options) => run_render(options),
            None => return usage_error(),
        },
        ["sheet", "--output", output_path] => run_sheet(Path::new(output_path)),
        _ => return usage_error(),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("git-bloom-activity: {message}");
            ExitCode::FAILURE
        }
    }
}

fn usage_error() -> ExitCode {
    eprintln!("{USAGE}");
    ExitCode::from(2)
}

struct RenderOptions {
    output_directory: PathBuf,
    repository_directory: PathBuf,
    repository: Option<String>,
    observed_on: Option<NaiveDate>,
}

fn parse_render_options(options: &[&str]) -> Option<RenderOptions> {
    let mut parsed = RenderOptions {
        output_directory: PathBuf::new(),
        repository_directory: PathBuf::from("."),
        repository: None,
        observed_on: None,
    };
    let mut has_output = false;
    for pair in options.chunks(2) {
        let [flag, value] = pair else { return None };
        match *flag {
            "--output-dir" => {
                parsed.output_directory = PathBuf::from(value);
                has_output = true;
            }
            "--repo-dir" => parsed.repository_directory = PathBuf::from(value),
            "--repository" if !value.is_empty() => parsed.repository = Some((*value).to_owned()),
            "--repository" => {}
            "--observed-on" => {
                parsed.observed_on = Some(NaiveDate::parse_from_str(value, "%Y-%m-%d").ok()?);
            }
            _ => return None,
        }
    }
    has_output.then_some(parsed)
}

fn run_render(options: RenderOptions) -> Result<(), String> {
    let repository = match options.repository {
        Some(repository) => repository,
        None => find_repository_name(&options.repository_directory)?,
    };
    let observed_on = options.observed_on.unwrap_or_else(yesterday_utc);
    let activity = collect_activity(&options.repository_directory, &repository, observed_on)?;
    let development = derive_development(&activity);
    let mood = select_mood(&development);
    let frames: Vec<BloomFrame> = (0..frame_count(mood))
        .map(|frame_index| render_frame(&development, mood, frame_index))
        .collect();
    let gif_bytes = encode_gif(&frames).map_err(|error| error.to_string())?;
    let png_bytes =
        encode_png(&frames[0]).map_err(|error| format!("PNG encoding failed: {error}"))?;
    let caption = format!(
        "{repository}: a {} from {} commits, {} on the day after {observed_on}",
        development.stage().name(),
        development.total_commits,
        mood.name(),
    );
    let directory = &options.output_directory;
    fs::create_dir_all(directory).map_err(|error| format!("{}: {error}", directory.display()))?;
    for (name, bytes) in [
        ("bloom.gif", gif_bytes),
        ("bloom.png", png_bytes),
        ("preview.html", encode_preview_html(&caption).into_bytes()),
    ] {
        replace_file(&directory.join(name), &bytes)?;
    }
    println!("{caption}");
    Ok(())
}

fn run_sheet(output_path: &Path) -> Result<(), String> {
    let mut frames = Vec::new();
    for total_commits in [0, 4, 18, 60, 400, 2_500, 10_000] {
        let development = sample_development(total_commits, 12, 30);
        frames.push(render_frame(&development, Mood::Sway, 0));
    }
    for mood in MOODS {
        let development = sample_development(1_200, 9, 60);
        let frame_index = match mood {
            Mood::Unfold => frame_count(mood) / 2,
            Mood::Wilt => frame_count(mood) / 3,
            _ => frame_count(mood) / 5,
        };
        frames.push(render_frame(&development, mood, frame_index));
    }
    for (seed, languages) in [
        (
            7,
            vec![
                ("Python", "#3572A5", 0.6),
                ("Jupyter Notebook", "#DA5B0B", 0.4),
            ],
        ),
        (8, vec![("Go", "#00ADD8", 1.0)]),
        (
            9,
            vec![("JavaScript", "#f1e05a", 0.7), ("CSS", "#663399", 0.3)],
        ),
    ] {
        let mut development = sample_development(800, 2, 3);
        development.identity_seed = seed;
        development.languages = languages
            .into_iter()
            .map(|(name, color, share)| PetalLanguage {
                name: name.into(),
                color: git_bloom_activity::activity::parse_hex_color(color),
                share,
            })
            .collect();
        frames.push(render_frame(&development, Mood::Sway, 0));
    }
    let bytes = encode_sheet_png(&frames, 5, [246, 248, 250, 255], 3)
        .map_err(|error| format!("PNG encoding failed: {error}"))?;
    replace_file(output_path, &bytes)
}

fn sample_development(total_commits: u64, current: u32, longest: u32) -> BloomDevelopment {
    BloomDevelopment {
        repository: "example/bloom".into(),
        identity_seed: 20_261_102,
        languages: [
            ("TypeScript", "#3178c6", 0.45),
            ("Rust", "#dea584", 0.3),
            ("Python", "#3572A5", 0.15),
            ("Shell", "#89e051", 0.1),
        ]
        .into_iter()
        .map(|(name, color, share)| PetalLanguage {
            name: name.into(),
            color: git_bloom_activity::activity::parse_hex_color(color),
            share,
        })
        .collect(),
        total_commits,
        observed_on: NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(),
        current_streak_weeks: current,
        longest_streak_weeks: longest,
        quiet_day_count: 0,
        recent_commits: 5,
        complete_week_commits: Vec::new(),
        observation_week_commits: vec![1],
    }
}

fn yesterday_utc() -> NaiveDate {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("the clock is after 1970")
        .as_secs() as i64;
    DateTime::from_timestamp(seconds, 0)
        .expect("the clock is in range")
        .date_naive()
        .pred_opt()
        .expect("today has a yesterday")
}

/// Write beside the destination, then rename over it, so a reader never sees half a file.
fn replace_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let temporary = path.with_extension("partial");
    fs::write(&temporary, bytes).map_err(|error| format!("{}: {error}", temporary.display()))?;
    fs::rename(&temporary, path).map_err(|error| format!("{}: {error}", path.display()))
}
