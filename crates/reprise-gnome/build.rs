//! Embeds the build's short git commit as `REPRISE_GIT_SHA` so the About
//! dialog can show which dev revision is running. Nightly builds export the
//! variable directly (authoritative); a plain `cargo build` falls back to
//! asking git. When neither is available the value is empty and About shows
//! only the crate version.

use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=REPRISE_GIT_SHA");

    let sha = std::env::var("REPRISE_GIT_SHA")
        .ok()
        .filter(|value| !value.is_empty())
        .or_else(git_short_sha)
        .unwrap_or_default();

    println!("cargo:rustc-env=REPRISE_GIT_SHA={sha}");
}

fn git_short_sha() -> Option<String> {
    let output = Command::new("git")
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
