//! Desktop diagnostic report wiring and session warning capture.

use std::fmt::Debug;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use chrono::{Local, Timelike};
use libadwaita as adw;
use reprise_core::db::Db;
use reprise_core::diagnostics::{
    is_safe_structured_field, DiagnosticEvent, DiagnosticLevel, DiagnosticLog,
};
use reprise_platform_linux::diagnostics::DesktopDiagnosticInput;
use tracing::field::{Field, Visit};
use tracing::{Event, Level, Metadata, Subscriber};
use tracing_subscriber::filter::{filter_fn, FilterFn};
use tracing_subscriber::layer::{Context, Layer};

type SharedDiagnosticLog = Arc<Mutex<DiagnosticLog>>;

static SESSION_LOG: OnceLock<SharedDiagnosticLog> = OnceLock::new();

pub(crate) fn session_layer() -> SessionDiagnosticLayer {
    SessionDiagnosticLayer::new(session_log())
}

pub(crate) fn session_filter() -> FilterFn {
    filter_fn(is_reprise_event)
}

fn is_reprise_event(metadata: &Metadata<'_>) -> bool {
    const TARGET_ROOTS: &[&str] = &[
        "reprise",
        "reprise_core",
        "reprise_platform_linux",
        "reprise_view",
    ];
    let root = metadata.target().split("::").next().unwrap_or_default();
    TARGET_ROOTS.contains(&root)
}

fn session_log() -> SharedDiagnosticLog {
    SESSION_LOG
        .get_or_init(|| Arc::new(Mutex::new(DiagnosticLog::default())))
        .clone()
}

pub(crate) fn build_report(db: &Db, db_path: &Path) -> String {
    let input = DesktopDiagnosticInput {
        version: env!("CARGO_PKG_VERSION").into(),
        git_sha: option_env!("REPRISE_GIT_SHA").map(str::to_string),
        build_profile: if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
        .into(),
        app_id: crate::APP_ID.into(),
        display_server: gtk4::gdk::Display::default().map(|_| {
            if super::compact::compact_mode_controls::is_x11() {
                "x11".into()
            } else {
                "wayland".into()
            }
        }),
        gtk_version: format!(
            "{}.{}.{}",
            gtk4::major_version(),
            gtk4::minor_version(),
            gtk4::micro_version()
        ),
        libadwaita_version: format!(
            "{}.{}.{}",
            adw::major_version(),
            adw::minor_version(),
            adw::micro_version()
        ),
        rust_version: option_env!("REPRISE_RUST_VERSION").map(str::to_string),
    };
    let log = session_log()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    reprise_platform_linux::diagnostics::build_report(db, db_path, &input, &log)
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
            let value = if is_safe_structured_field(field.name()) {
                value
            } else {
                "$REDACTED".into()
            };
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

#[cfg(test)]
mod tests;
