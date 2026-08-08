//! Embeds two compile-time values.
//!
//! `REPRISE_GIT_SHA` is the build's short git commit so the About dialog can
//! show which dev revision is running. Nightly builds export the variable
//! directly (authoritative); a plain `cargo build` falls back to asking git.
//! When neither is available the value is empty and About shows only the
//! crate version.
//!
//! `REPRISE_APP_ACCENT` is read out of `data/brand/palette.toml`, the single
//! maintained source for every brand colour. Lifting it here rather than
//! restating it in Rust keeps that promise while leaving the accent a plain
//! `&'static str` constant — CSS construction at startup needs it before any
//! file could be read.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The palette key holding Reprise's brand accent.
const ACCENT_KEY: &str = "reprise_teal";

fn main() {
    println!("cargo:rerun-if-env-changed=REPRISE_GIT_SHA");
    emit_git_rerun_paths();

    let sha = std::env::var("REPRISE_GIT_SHA")
        .ok()
        .filter(|value| !value.is_empty())
        .or_else(git_short_sha)
        .unwrap_or_default();

    println!("cargo:rustc-env=REPRISE_GIT_SHA={sha}");

    let palette = palette_path();
    println!("cargo:rerun-if-changed={}", palette.display());
    println!(
        "cargo:rustc-env=REPRISE_APP_ACCENT={}",
        read_palette_color(&palette, ACCENT_KEY)
    );
}

fn palette_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/brand/palette.toml")
}

/// Extracts one `key = "#RRGGBB"` entry from the flat brand palette. Hand
/// parsing keeps the build free of a TOML dependency for a five-line file;
/// every failure aborts the build rather than guessing a colour.
fn read_palette_color(path: &Path, key: &str) -> String {
    let text = std::fs::read_to_string(path).unwrap_or_else(|error| {
        panic!("cannot read the brand palette {}: {error}", path.display())
    });

    let Some((_, declared)) = text
        .lines()
        .filter_map(|line| line.split_once('='))
        .find(|(name, _)| name.trim() == key)
    else {
        panic!("the brand palette {} declares no {key}", path.display());
    };

    let value = declared.trim().trim_matches('"').to_string();
    assert!(
        is_hex_color(&value),
        "the brand palette {} declares {key} = {value:?}, which is not #RRGGBB",
        path.display()
    );
    value
}

fn is_hex_color(value: &str) -> bool {
    value.len() == 7
        && value.starts_with('#')
        && value[1..].bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Makes Cargo invalidate the embedded fallback SHA when this checkout moves.
/// A linked worktree keeps `HEAD` in its per-worktree git directory while the
/// symbolic branch ref lives in the shared common directory.
fn emit_git_rerun_paths() {
    let Some(git_dir) = git_metadata_dir("--git-dir") else {
        return;
    };
    let head = git_dir.join("HEAD");
    println!("cargo::rerun-if-changed={}", head.display());

    let Ok(contents) = std::fs::read_to_string(&head) else {
        return;
    };
    let Some(reference) = contents.strip_prefix("ref: ").map(str::trim) else {
        return;
    };
    let Some(common_dir) = git_metadata_dir("--git-common-dir") else {
        return;
    };
    println!(
        "cargo::rerun-if-changed={}",
        common_dir.join(reference).display()
    );
}

fn git_metadata_dir(argument: &str) -> Option<PathBuf> {
    let output = Command::new("git")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["rev-parse", argument])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let path = PathBuf::from(String::from_utf8(output.stdout).ok()?.trim());
    if path.as_os_str().is_empty() {
        return None;
    }
    Some(if path.is_absolute() {
        path
    } else {
        Path::new(env!("CARGO_MANIFEST_DIR")).join(path)
    })
}

fn git_short_sha() -> Option<String> {
    let output = Command::new("git")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let sha = String::from_utf8(output.stdout).ok()?;
    let sha = sha.trim();
    (!sha.is_empty()).then(|| sha.to_string())
}
