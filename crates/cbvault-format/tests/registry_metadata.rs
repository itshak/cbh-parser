//! Checks the metadata crates.io enforces at upload time, locally and early.
//!
//! crates.io validates a crate's manifest only when it is published, which for
//! this workspace means only on a release tag. One of these limits was found
//! that way: an over-long keyword list rejected the `v0.1.0` upload with
//! `expected at most 5 keywords per crate`, after the tests and the package
//! check had both passed. Everything here is therefore checked in CI on the
//! branch, where a mistake is a pull request rather than a moved tag.

use std::path::{Path, PathBuf};

/// crates.io's documented maximum for `keywords`.
const MAX_KEYWORDS: usize = 5;
/// The shortest and longest `description` crates.io accepts.
const DESC_RANGE: std::ops::RangeInclusive<usize> = 10..=200;
/// crates.io's maximum for `categories`.
const MAX_CATEGORIES: usize = 5;

fn workspace_root() -> PathBuf {
    // CARGO_MANIFEST_DIR is crates/cbvault-format; the workspace root is two up.
    Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2).expect("crate is inside the workspace").to_path_buf()
}

/// Every workspace crate that is actually published, i.e. not `publish = false`.
fn published_crates() -> Vec<PathBuf> {
    let root = workspace_root();
    let mut out: Vec<PathBuf> = std::fs::read_dir(root.join("crates"))
        .expect("crates/ exists")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .map(|p| p.join("Cargo.toml"))
        .filter(|p| p.exists())
        .filter(|p| !std::fs::read_to_string(p).unwrap_or_default().contains("publish = false"))
        .collect();
    out.sort();
    assert!(!out.is_empty(), "found no publishable crates");
    out
}

/// Reads a string array from a `[package]` key, without a TOML dependency.
fn array_field(manifest: &str, key: &str) -> Vec<String> {
    let line = manifest
        .lines()
        .find(|l| l.starts_with(key) && l.contains('='))
        .unwrap_or_else(|| panic!("no `{key}` in manifest"));
    let inner = line.split_once('=').unwrap().1.trim().trim_start_matches('[').trim_end_matches(']');
    inner.split(',').map(|s| s.trim().trim_matches('"').to_string()).filter(|s| !s.is_empty()).collect()
}

fn scalar_field(manifest: &str, key: &str) -> String {
    manifest
        .lines()
        .find(|l| l.starts_with(key) && l.contains('='))
        .map(|l| l.split_once('=').unwrap().1.trim().trim_matches('"').to_string())
        .unwrap_or_default()
}

#[test]
fn every_crate_has_at_most_five_keywords() {
    for manifest in published_crates() {
        let text = std::fs::read_to_string(&manifest).unwrap();
        let keywords = array_field(&text, "keywords");
        assert!(
            keywords.len() <= MAX_KEYWORDS,
            "{} has {} keywords, crates.io allows at most {MAX_KEYWORDS}: {keywords:?}\n\
             Trim it and say in a comment which were dropped and why.",
            manifest.display(),
            keywords.len()
        );
    }
}

#[test]
fn every_crate_description_is_within_the_accepted_length() {
    for manifest in published_crates() {
        let text = std::fs::read_to_string(&manifest).unwrap();
        let desc = scalar_field(&text, "description");
        assert!(
            DESC_RANGE.contains(&desc.chars().count()),
            "{}: description is {} chars, crates.io accepts {}..={}",
            manifest.display(),
            desc.chars().count(),
            DESC_RANGE.start(),
            DESC_RANGE.end()
        );
    }
}

#[test]
fn every_crate_has_keywords_categories_and_a_license() {
    // crates.io does not require these, but a crate without them is
    // effectively unsearchable, which is not a state worth shipping.
    for manifest in published_crates() {
        let text = std::fs::read_to_string(&manifest).unwrap();
        for key in ["keywords", "categories"] {
            assert!(
                !array_field(&text, key).is_empty(),
                "{} declares no {key}; it will be unsearchable on crates.io",
                manifest.display()
            );
        }
        assert!(
            array_field(&text, "categories").len() <= MAX_CATEGORIES,
            "{} has more than {MAX_CATEGORIES} categories",
            manifest.display()
        );
        assert!(
            !scalar_field(&text, "license").is_empty(),
            "{} declares no license; the workspace default is MIT",
            manifest.display()
        );
    }
}

#[test]
fn every_crate_readme_path_exists() {
    // A `readme` path that does not exist fails the upload, and only the upload.
    for manifest in published_crates() {
        let text = std::fs::read_to_string(&manifest).unwrap();
        let readme = scalar_field(&text, "readme");
        if readme.is_empty() {
            continue; // inherits the workspace README
        }
        let dir = manifest.parent().unwrap();
        assert!(dir.join(&readme).exists(), "{} points readme at {readme:?}, which does not exist", manifest.display());
    }
}

#[test]
fn only_the_cli_claims_the_command_line_category() {
    // crates.io rejects `command-line-utilities` on a crate with no binary
    // target, so this is asserted rather than left for the upload to discover.
    for manifest in published_crates() {
        let text = std::fs::read_to_string(&manifest).unwrap();
        if !array_field(&text, "categories").iter().any(|c| c == "command-line-utilities") {
            continue;
        }
        let name = scalar_field(&text, "name");
        let has_bin = text.contains("[[bin]]") || manifest.parent().is_some_and(|d| d.join("src/main.rs").exists());
        assert!(has_bin, "{name} claims `command-line-utilities` but declares no binary target");
        assert_eq!(name, "cbvault-cli", "{name} also claims it");
    }
}
