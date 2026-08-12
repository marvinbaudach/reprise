//! Fallible, read-only diagnostic collection owned by the Linux platform boundary.

use std::path::Path;
use std::process::Command;

use gio::glib;
use gstreamer as gst;
use reprise_core::db::Db;
use reprise_core::diagnostics::{
    render_report, DiagnosticFacts, DiagnosticLog, PackageKind, RedactionContext,
};

/// Runtime values that only the native frontend can observe directly.
pub struct DesktopDiagnosticInput {
    pub version: String,
    pub git_sha: Option<String>,
    pub build_profile: String,
    pub app_id: String,
    pub display_server: Option<String>,
    pub gtk_version: String,
    pub libadwaita_version: String,
    pub rust_version: Option<String>,
}

/// Collects fallible Linux and database facts and renders a privacy-safe report.
pub fn build_report(
    db: &Db,
    db_path: &Path,
    input: &DesktopDiagnosticInput,
    log: &DiagnosticLog,
) -> String {
    let facts = collect_facts(db, db_path, input);
    render_report(&facts, log, &redaction_context())
}

fn collect_facts(db: &Db, db_path: &Path, input: &DesktopDiagnosticInput) -> DiagnosticFacts {
    let os_release = std::fs::read_to_string("/etc/os-release")
        .ok()
        .map(|contents| parse_os_release(&contents))
        .unwrap_or_default();
    let db_facts = db.diagnostic_facts().ok();
    let stats = reprise_core::queries::query_library_stats(db, "").ok();
    let remembered_device_count = reprise_core::device_sync::settings::list_remembered_devices(db)
        .ok()
        .map(|devices| devices.len());

    DiagnosticFacts {
        version: Some(input.version.clone()),
        git_sha: input.git_sha.clone().filter(|value| !value.is_empty()),
        build_profile: Some(input.build_profile.clone()),
        package: Some(if Path::new("/.flatpak-info").is_file() {
            PackageKind::Flatpak {
                app_id: Some(input.app_id.clone()),
            }
        } else {
            PackageKind::Native
        }),
        os_name: os_release.name,
        os_version: os_release.version,
        gnome_version: command_output("gnome-shell", &["--version"])
            .and_then(|output| parse_gnome_version(&output)),
        display_server: input.display_server.clone(),
        gtk_version: Some(input.gtk_version.clone()),
        libadwaita_version: Some(input.libadwaita_version.clone()),
        rust_version: input.rust_version.clone().filter(|value| !value.is_empty()),
        gstreamer_version: gstreamer_version(),
        audio_backend: active_audio_backend(),
        locale: locale(),
        db_schema: db_facts.as_ref().map(|facts| facts.schema_version),
        db_journal_mode: db_facts.map(|facts| facts.journal_mode),
        track_count: stats.map(|stats| stats.track_count),
        db_size_bytes: std::fs::metadata(db_path)
            .ok()
            .map(|metadata| metadata.len()),
        gvfs_version: gvfs_version(),
        remembered_device_count,
    }
}

fn redaction_context() -> RedactionContext {
    RedactionContext {
        music_dir: glib::user_special_dir(glib::UserDirectory::Music)
            .map(|path| path.to_string_lossy().into_owned()),
        home_dir: std::env::var("HOME").ok().filter(|value| !value.is_empty()),
        username: std::env::var("USER").ok().filter(|value| !value.is_empty()),
    }
}

fn locale() -> Option<String> {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .into_iter()
        .find_map(|name| std::env::var(name).ok().filter(|value| !value.is_empty()))
}

fn gvfs_version() -> Option<String> {
    ["gvfsd", "/usr/libexec/gvfsd", "/usr/lib/gvfsd"]
        .into_iter()
        .find_map(|program| {
            command_output(program, &["--version"]).and_then(|output| parse_gvfs_version(&output))
        })
}

fn command_output(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let output = String::from_utf8(output.stdout).ok()?;
    nonempty(Some(output.trim()))
}

fn nonempty(value: Option<&str>) -> Option<String> {
    value.filter(|value| !value.is_empty()).map(str::to_string)
}

fn gstreamer_version() -> Option<String> {
    gst::init().ok()?;
    let (major, minor, micro, _) = gst::version();
    Some(format!("{major}.{minor}.{micro}"))
}

/// Reprise can name the selected sink only when its explicit override is in use.
/// GStreamer's automatic sink is resolved inside playbin and is otherwise left
/// unknown rather than guessed from the factories installed on the host.
fn active_audio_backend() -> Option<String> {
    std::env::var("REPRISE_AUDIO_SINK")
        .ok()
        .filter(|value| !value.is_empty())
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct OsRelease {
    name: Option<String>,
    version: Option<String>,
}

fn parse_os_release(contents: &str) -> OsRelease {
    let value = |key: &str| {
        contents.lines().find_map(|line| {
            let (name, value) = line.split_once('=')?;
            (name == key).then(|| value.trim().trim_matches(['"', '\'']).to_string())
        })
    };
    OsRelease {
        name: value("ID"),
        version: value("VERSION_ID"),
    }
}

fn parse_gnome_version(output: &str) -> Option<String> {
    output
        .split_whitespace()
        .find(|part| part.chars().next().is_some_and(|ch| ch.is_ascii_digit()))
        .map(str::to_string)
}

fn parse_gvfs_version(output: &str) -> Option<String> {
    output
        .split_whitespace()
        .find(|part| part.chars().next().is_some_and(|ch| ch.is_ascii_digit()))
        .map(str::to_string)
}

#[cfg(test)]
mod tests;
