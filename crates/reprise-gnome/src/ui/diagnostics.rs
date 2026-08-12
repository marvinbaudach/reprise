//! Desktop runtime-fact collection and session warning capture.

use std::fmt::Debug;
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use chrono::{Local, Timelike};
use gtk4::glib;
use libadwaita as adw;
use reprise_core::db::Db;
use reprise_core::diagnostics::{
    render_report, DiagnosticEvent, DiagnosticFacts, DiagnosticLevel, DiagnosticLog, PackageKind,
    RedactionContext,
};
use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::layer::{Context, Layer};

type SharedDiagnosticLog = Arc<Mutex<DiagnosticLog>>;

static SESSION_LOG: OnceLock<SharedDiagnosticLog> = OnceLock::new();

pub(crate) fn session_layer() -> SessionDiagnosticLayer {
    SessionDiagnosticLayer::new(session_log())
}

fn session_log() -> SharedDiagnosticLog {
    SESSION_LOG
        .get_or_init(|| Arc::new(Mutex::new(DiagnosticLog::default())))
        .clone()
}

pub(crate) fn build_report(db: &Db, db_path: &Path) -> String {
    let facts = collect_facts(db, db_path);
    let redaction = redaction_context();
    let log = session_log()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    render_report(&facts, &log, &redaction)
}

fn collect_facts(db: &Db, db_path: &Path) -> DiagnosticFacts {
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
        version: Some(env!("CARGO_PKG_VERSION").into()),
        git_sha: nonempty(option_env!("REPRISE_GIT_SHA")),
        build_profile: Some(
            if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            }
            .into(),
        ),
        package: Some(if Path::new("/.flatpak-info").is_file() {
            PackageKind::Flatpak {
                app_id: Some(crate::APP_ID.into()),
            }
        } else {
            PackageKind::Native
        }),
        os_name: os_release.name,
        os_version: os_release.version,
        gnome_version: command_output("gnome-shell", &["--version"])
            .and_then(|output| parse_gnome_version(&output)),
        display_server: gtk4::gdk::Display::default().map(|_| {
            if super::compact::compact_mode_controls::is_x11() {
                "x11".into()
            } else {
                "wayland".into()
            }
        }),
        gtk_version: Some(format!(
            "{}.{}.{}",
            gtk4::major_version(),
            gtk4::minor_version(),
            gtk4::micro_version()
        )),
        libadwaita_version: Some(format!(
            "{}.{}.{}",
            adw::major_version(),
            adw::minor_version(),
            adw::micro_version()
        )),
        rust_version: nonempty(option_env!("REPRISE_RUST_VERSION")),
        gstreamer_version: reprise_platform_linux::diagnostics::gstreamer_version(),
        audio_backend: reprise_platform_linux::diagnostics::active_audio_backend(),
        locale: locale(),
        db_schema: db_facts.as_ref().map(|facts| facts.schema_version),
        db_journal_mode: db_facts.map(|facts| facts.journal_mode),
        track_count: stats.map(|stats| stats.track_count),
        db_size_bytes: std::fs::metadata(db_path)
            .ok()
            .map(|metadata| metadata.len()),
        libmtp_version: command_output("pkg-config", &["--modversion", "libmtp"]),
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

#[derive(Clone)]
pub(crate) struct SessionDiagnosticLayer {
    log: SharedDiagnosticLog,
}

impl SessionDiagnosticLayer {
    fn new(log: SharedDiagnosticLog) -> Self {
        Self { log }
    }
}

impl<S> Layer<S> for SessionDiagnosticLayer
where
    S: Subscriber,
{
    fn on_event(&self, event: &Event<'_>, _context: Context<'_, S>) {
        let level = match *event.metadata().level() {
            Level::WARN => DiagnosticLevel::Warn,
            Level::ERROR => DiagnosticLevel::Error,
            _ => return,
        };
        let mut visitor = EventFields::default();
        event.record(&mut visitor);
        let now = Local::now();
        let seconds_since_midnight = now.num_seconds_from_midnight();
        let diagnostic = DiagnosticEvent::new(
            seconds_since_midnight,
            level,
            event.metadata().target(),
            visitor.into_message(),
        );
        self.log
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(diagnostic);
    }
}

#[derive(Default)]
struct EventFields {
    message: Option<String>,
    fields: Vec<String>,
}

impl EventFields {
    fn into_message(self) -> String {
        match (self.message, self.fields.is_empty()) {
            (Some(message), true) => message,
            (Some(message), false) => format!("{message}; {}", self.fields.join(" ")),
            (None, false) => self.fields.join(" "),
            (None, true) => "warning without details".into(),
        }
    }

    fn record_value(&mut self, field: &Field, value: String) {
        if field.name() == "message" {
            self.message = Some(value);
        } else {
            self.fields.push(format!("{}={value}", field.name()));
        }
    }
}

impl Visit for EventFields {
    fn record_debug(&mut self, field: &Field, value: &dyn Debug) {
        self.record_value(field, format!("{value:?}"));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.record_value(field, value.to_string());
    }
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

#[cfg(test)]
mod tests;
