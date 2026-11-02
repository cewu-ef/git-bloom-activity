//! Grow a pixel-art flower from one repository's commit history.
//!
//! The pipeline runs one way, and each module owns one stage of it:
//!
//! 1. [`collect`] reads the repository's commits from git and its languages from GitHub
//!    into a [`activity::RepositoryActivity`].
//! 2. [`growth`] turns that into a [`growth::BloomDevelopment`]: identity, language
//!    shares, size, weekly streaks, and recent activity.
//! 3. [`mood`] chooses the day's [`mood::Mood`] from recent activity.
//! 4. [`render`] draws a [`render::BloomFrame`] for each frame of the mood's loop, and
//!    [`output`] encodes frames as a PNG, a looping GIF, and a preview page.
//!
//! Everything after collection is deterministic: it reads no clock and performs no I/O.
#![warn(missing_docs)]

pub mod activity;
pub mod collect;
pub mod growth;
pub mod mood;
pub mod output;
pub mod render;
