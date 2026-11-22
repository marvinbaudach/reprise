//! Desktop diagnostic report wiring and session warning capture.

use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use chrono::{Local, Timelike};
use libadwaita as adw;
use reprise_core::db::Db;
use reprise_core::diagnostics::{read_log_tail, DiagnosticEvent, DiagnosticLevel, DiagnosticLog};
use reprise_core::state_root;
use reprise_platform_linux::diagnostics::DesktopDiagnosticInput;
use tracing::{Event, Level, Metadata, Subscriber};
use tracing_subscriber::filter::{filter_fn, FilterFn};
use tracing_subscriber::layer::{Context, Layer};

pub(crate) mod copy;
pub(crate) mod crash_offer;
mod event_record;
pub(crate) mod file_log;
pub(crate) mod file_mirror;

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
        log_tail: state_root::log_path().and_then(|path| read_log_tail(&path)),
        previous_log_tail: state_root::previous_log_path().and_then(|path| read_log_tail(&path)),
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
        let safe = event_record::safe_of(event);
        let seconds_since_midnight = Local::now().num_seconds_from_midnight();
        // What is stored is already safe: the report embeds it as it is.
        let diagnostic =
            DiagnosticEvent::new(seconds_since_midnight, level, safe.target(), safe.body())
                .with_location(safe.location());
        self.log
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(diagnostic);
    }
}

#[cfg(test)]
mod run_log_fixture_tests;
#[cfg(test)]
mod tests;
