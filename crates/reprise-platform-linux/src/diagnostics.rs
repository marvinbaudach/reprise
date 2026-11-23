//! Fallible, read-only diagnostic collection owned by the Linux platform boundary.

use std::path::Path;
use std::process::Command;

use gstreamer as gst;
use reprise_core::db::Db;
use reprise_core::diagnostics::{render_report, DiagnosticFacts, DiagnosticLog, PackageKind};

use crate::flatpak::FLATPAK_INFO;

const GNOME_PLATFORM: &str = "org.gnome.Platform";

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
    /// The end of this run's log file; read by the caller, which owns the state directory.
    pub log_tail: Option<String>,
    /// The end of the previous run's log file.
    pub previous_log_tail: Option<String>,
}

/// Collects fallible Linux and database facts and renders a privacy-safe report.
pub fn build_report(
    db: &Db,
    db_path: &Path,
    input: &DesktopDiagnosticInput,
    log: &DiagnosticLog,
) -> String {
    let facts = collect_facts(db, db_path, input);
    render_report(&facts, log)
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
        package: Some(if Path::new(FLATPAK_INFO).is_file() {
            PackageKind::Flatpak {
                app_id: Some(input.app_id.clone()),
            }
        } else {
            PackageKind::Native
        }),
        os_name: os_release.name,
        os_version: os_release.version,
        gnome_version: gnome_version_from(
            command_output("gnome-shell", &["--version"])
                .and_then(|output| parse_gnome_version(&output))
                .as_deref(),
            flatpak_gnome_runtime().as_deref(),
        ),
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
        enabled_modules: enabled_module_ids(db),
        log_tail: input.log_tail.clone(),
        previous_log_tail: input.previous_log_tail.clone(),
    }
}

/// Ids of the optional modules that are switched on. A module whose setting
/// cannot be read makes the whole list unknown rather than silently absent.
fn enabled_module_ids(db: &Db) -> Option<Vec<String>> {
    let mut ids = Vec::new();
    for module in reprise_core::modules::ALL_MODULES {
        if reprise_core::modules::is_enabled(db, module).ok()? {
            ids.push(module.id.to_string());
        }
    }
    Some(ids)
}

fn locale() -> Option<String> {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .into_iter()
        .find_map(|name| std::env::var(name).ok().filter(|value| !value.is_empty()))
        .map(|value| plain_locale(&value))
}

/// What the report shows when an environment value is not in the shape it is
/// meant to have.
const UNKNOWN_FACT: &str = "unknown";

/// `value` when it is a locale name, `unknown` otherwise: the language (two or
/// three lower-case letters, or `C`/`POSIX`), then optionally `_` and a
/// two-letter upper-case territory, `.` and a known codeset, `@` and a known
/// modifier. The environment is the user's, so a value of any other shape,
/// including free text in the codeset or modifier position, is not copied
/// into a report.
fn plain_locale(value: &str) -> String {
    if is_locale_shape(value) {
        value.to_owned()
    } else {
        UNKNOWN_FACT.to_owned()
    }
}

/// The locale modifiers glibc ships. Any other text after `@` is the user's.
const KNOWN_LOCALE_MODIFIERS: [&str; 7] = [
    "cyrillic",
    "devanagari",
    "euro",
    "iqtelif",
    "latin",
    "saaho",
    "valencia",
];

/// The codesets a locale name carries in practice, compared without regard
/// to case: UTF-8, ISO-8859-1 to -16 (with or without the dashes), GB18030,
/// EUC-JP and KOI8-R. Any other text after `.` is the user's.
fn is_known_codeset(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    if matches!(
        lower.as_str(),
        "utf-8" | "utf8" | "gb18030" | "euc-jp" | "koi8-r"
    ) {
        return true;
    }
    let part = lower
        .strip_prefix("iso-8859-")
        .or_else(|| lower.strip_prefix("iso8859"))
        .map(|part| part.trim_start_matches('-'));
    part.is_some_and(|digits| {
        digits.bytes().all(|byte| byte.is_ascii_digit())
            && digits
                .parse::<u8>()
                .is_ok_and(|number| (1..=16).contains(&number))
    })
}

fn is_locale_shape(value: &str) -> bool {
    let (rest, modifier) = value
        .split_once('@')
        .map_or((value, None), |(rest, modifier)| (rest, Some(modifier)));
    let (rest, codeset) = rest
        .split_once('.')
        .map_or((rest, None), |(rest, codeset)| (rest, Some(codeset)));
    let (language, territory) = rest
        .split_once('_')
        .map_or((rest, None), |(language, territory)| {
            (language, Some(territory))
        });
    let modifier_fits = modifier.is_none_or(|text| KNOWN_LOCALE_MODIFIERS.contains(&text));
    let codeset_fits = codeset.is_none_or(is_known_codeset);
    let territory_fits = territory
        .is_none_or(|text| text.len() == 2 && text.bytes().all(|byte| byte.is_ascii_uppercase()));
    let language_fits = if matches!(language, "C" | "POSIX") {
        territory.is_none() && modifier.is_none()
    } else {
        (2..=3).contains(&language.len()) && language.bytes().all(|byte| byte.is_ascii_lowercase())
    };
    modifier_fits && codeset_fits && territory_fits && language_fits
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
        .map(|value| plain_audio_sink(&value).to_owned())
}

/// The GStreamer sink factories the override is documented to take. Any other
/// value can be a pipeline description or a path, so it is shown as `custom`.
const KNOWN_AUDIO_SINKS: [&str; 5] = [
    "alsasink",
    "autoaudiosink",
    "fakesink",
    "pipewiresink",
    "pulsesink",
];

/// The sink name when it is one of [`KNOWN_AUDIO_SINKS`], `custom` otherwise.
fn plain_audio_sink(value: &str) -> &'static str {
    KNOWN_AUDIO_SINKS
        .into_iter()
        .find(|known| *known == value)
        .unwrap_or("custom")
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
        version: value("VERSION_ID")
            .filter(|version| !version.is_empty())
            .or_else(|| value("BUILD_ID").filter(|version| !version.is_empty())),
    }
}

fn parse_gnome_version(output: &str) -> Option<String> {
    output
        .split_whitespace()
        .find(|part| part.chars().next().is_some_and(|ch| ch.is_ascii_digit()))
        .map(str::to_string)
}

/// The shell's own version when `gnome-shell` runs, else the GNOME runtime the
/// sandbox ships. A Flatpak cannot execute the host's shell, so without the
/// fallback every sandboxed report said `gnome unknown`.
fn gnome_version_from(shell: Option<&str>, flatpak_runtime: Option<&str>) -> Option<String> {
    shell.or(flatpak_runtime).map(str::to_string)
}

fn flatpak_gnome_runtime() -> Option<String> {
    let info = std::fs::read_to_string(FLATPAK_INFO).ok()?;
    parse_flatpak_gnome_runtime(&info)
}

/// The GNOME platform version from the `[Application]` section of
/// `/.flatpak-info` (`runtime=runtime/org.gnome.Platform/<arch>/<version>`),
/// labelled so it is not mistaken for the host shell.
fn parse_flatpak_gnome_runtime(info: &str) -> Option<String> {
    let mut in_application = false;
    for line in info.lines().map(str::trim) {
        if line.starts_with('[') {
            in_application = line == "[Application]";
        } else if in_application {
            let Some(("runtime", reference)) = line.split_once('=') else {
                continue;
            };
            let mut parts = reference.split('/');
            let version = match (parts.next(), parts.next(), parts.next(), parts.next()) {
                (Some("runtime"), Some(GNOME_PLATFORM), Some(_arch), Some(version)) => version,
                _ => return None,
            };
            return (!version.is_empty()).then(|| format!("{version} (Flatpak runtime)"));
        }
    }
    None
}

fn parse_gvfs_version(output: &str) -> Option<String> {
    output
        .split_whitespace()
        .find(|part| part.chars().next().is_some_and(|ch| ch.is_ascii_digit()))
        .map(str::to_string)
}

#[cfg(test)]
mod tests;
