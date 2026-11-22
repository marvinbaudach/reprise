//! Events through both desktop layers into the copied report.
//!
//! One subscriber carries the session layer (the "last warnings" block, without
//! its target filter, which `Event::dispatch` would not consult) and
//! the file mirror (the run log tail). An own event needs a call site the
//! template table knows, and test code is not in the table, so the events here
//! are dispatched with hand-built static metadata that names a production
//! source file, as a real call site's metadata would. The production site of
//! `RunLog::start_in` joins them, so the real macro path is covered too.

use std::sync::{Arc, Mutex};

use reprise_core::diagnostics::{
    render_report, templates_for_file, DiagnosticFacts, DiagnosticLog, LogTemplate,
};
use tracing::callsite::{DefaultCallsite, Identifier};
use tracing::field::{debug, FieldSet, Value};
use tracing::metadata::Kind;
use tracing::{Event, Level, Metadata};
use tracing_subscriber::layer::SubscriberExt;

use super::super::SessionDiagnosticLayer;
use super::FileMirrorLayer;

const OWN_FILE: &str = "crates/reprise-gnome/src/ui/sidebar/sidebar_rebuild.rs";
const OWN_TARGET: &str = "reprise::ui::sidebar";
const FOREIGN_TARGET: &str = "foreign_transport::connection";
const NEEDLE: &str = "zqx";

/// The fields of the hand-built call sites, in the order values are given.
const FIELD_NAMES: &[&str] = &["message", "track_id", "count", "state", "path", "error"];

static OWN_WARN_CALLSITE: DefaultCallsite = DefaultCallsite::new(&OWN_WARN);
static OWN_WARN: Metadata<'static> = Metadata::new(
    "event",
    OWN_TARGET,
    Level::WARN,
    Some(OWN_FILE),
    Some(30),
    Some(OWN_TARGET),
    FieldSet::new(FIELD_NAMES, Identifier(&OWN_WARN_CALLSITE)),
    Kind::EVENT,
);

static FOREIGN_WARN_CALLSITE: DefaultCallsite = DefaultCallsite::new(&FOREIGN_WARN);
static FOREIGN_WARN: Metadata<'static> = Metadata::new(
    "event",
    FOREIGN_TARGET,
    Level::WARN,
    Some("/cargo/registry/src/dependency-1.0.0/src/lib.rs"),
    Some(9),
    Some(FOREIGN_TARGET),
    FieldSet::new(FIELD_NAMES, Identifier(&FOREIGN_WARN_CALLSITE)),
    Kind::EVENT,
);

/// The values of one event, by field. `None` leaves the field out.
#[derive(Default)]
struct Values<'a> {
    track_id: Option<i64>,
    count: Option<u64>,
    state: Option<&'a str>,
    path: Option<&'a str>,
    error: Option<&'a str>,
}

fn dispatch(metadata: &'static Metadata<'static>, message: &str, values: &Values<'_>) {
    let message = format_args!("{message}");
    let error = values.error.map(debug);
    let entries: [Option<&dyn Value>; 6] = [
        Some(&message),
        values.track_id.as_ref().map(|value| value as &dyn Value),
        values.count.as_ref().map(|value| value as &dyn Value),
        values.state.as_ref().map(|value| value as &dyn Value),
        values.path.as_ref().map(|value| value as &dyn Value),
        error.as_ref().map(|value| value as &dyn Value),
    ];
    Event::dispatch(metadata, &metadata.fields().value_set_all(&entries));
}

/// Runs `log` under a subscriber with both desktop layers and returns the
/// session log and the text the file mirror wrote.
fn through_both_layers(log: impl FnOnce()) -> (DiagnosticLog, String) {
    let session = Arc::new(Mutex::new(DiagnosticLog::default()));
    let file = Arc::new(Mutex::new(String::new()));
    let sink = file.clone();
    let subscriber = tracing_subscriber::registry()
        .with(SessionDiagnosticLayer::new(session.clone()))
        .with(FileMirrorLayer::new(move |line: &str| {
            sink.lock().unwrap().push_str(line);
        }));
    tracing::subscriber::with_default(subscriber, log);
    let events = session.lock().unwrap().clone();
    let text = file.lock().unwrap().clone();
    (events, text)
}

/// The first template of the sidebar file without a hole, and the first with
/// one: a static message to log as written and a holed one to log with a value.
fn static_and_holed_templates() -> (String, LogTemplate) {
    let templates = templates_for_file(OWN_FILE);
    let plain = templates
        .iter()
        .find(|template| template.hole_count() == 0)
        .map(LogTemplate::reduced)
        .unwrap();
    let holed = templates
        .iter()
        .find(|template| template.hole_count() > 0)
        .cloned()
        .unwrap();
    (plain, holed)
}

/// The message `template` renders to when every hole prints `value`.
fn rendered_with(template: &LogTemplate, value: &str) -> String {
    template
        .canonical()
        .replace("{{", "\u{0}")
        .replace("}}", "\u{1}")
        .replace("{}", value)
        .replace('\u{0}', "{")
        .replace('\u{1}', "}")
}

/// What the call sites here put into the events, nothing of which may reach
/// the report except the typed numbers.
const SECRET_PATH: &str = "/home/zqxuser/Music/Zqx Artist/Zqx Album/01 Zqx.flac";
const SECRET_ERROR: &str = "No such file or directory: /srv/zqxshare/Zqx Album";
const SECRET_STATE: &str = "Zqx Artist - Zqx Song";

fn log_all_kinds_of_event() {
    let (plain, holed) = static_and_holed_templates();
    dispatch(
        &OWN_WARN,
        &plain,
        &Values {
            track_id: Some(7),
            count: Some(3),
            state: Some(SECRET_STATE),
            path: Some(SECRET_PATH),
            error: Some(SECRET_ERROR),
        },
    );
    dispatch(
        &OWN_WARN,
        &rendered_with(&holed, SECRET_PATH),
        &Values {
            track_id: Some(8),
            ..Values::default()
        },
    );
    dispatch(
        &FOREIGN_WARN,
        &format!("could not read {SECRET_PATH} for zqxuser"),
        &Values {
            track_id: Some(9),
            path: Some(SECRET_PATH),
            ..Values::default()
        },
    );
    dispatch(
        &OWN_WARN,
        &format!("a message no template of the file has: {SECRET_STATE}"),
        &Values {
            count: Some(4),
            ..Values::default()
        },
    );
    // A real production call site: the run log fails to move a directory
    // aside, and its error names the temporary directory.
    let root = tempfile::Builder::new()
        .prefix("zqx-state-")
        .tempdir()
        .unwrap();
    let previous = reprise_core::state_root::previous_log_path_in(root.path());
    std::fs::create_dir_all(&previous).unwrap();
    let _ = reprise_core::run_log::RunLog::start_in(root.path());
}

#[test]
fn fb_19_events_through_both_layers_reach_the_report_as_static_text_and_typed_numbers() {
    let (plain, holed) = static_and_holed_templates();
    let (session, file_text) = through_both_layers(log_all_kinds_of_event);
    let facts = DiagnosticFacts {
        log_tail: Some(file_text.clone()),
        ..DiagnosticFacts::default()
    };

    let report = render_report(&facts, &session);

    // The static message is verbatim, with its typed numbers, in both blocks.
    let first =
        format!("{plain}; track_id=7 count=3 state=$REDACTED path=$REDACTED error=$REDACTED");
    let at = format!(" @ {OWN_FILE}:30");
    assert_eq!(report.matches(&first).count(), 2, "{report}");
    assert_eq!(
        report.matches(&format!("{first}{at}")).count(),
        2,
        "{report}"
    );
    // The holed message shows its template with `…` and keeps its number.
    let second = format!("{}; track_id=8{at}", holed.reduced());
    assert_eq!(report.matches(&second).count(), 2, "{report}");
    // An own message no template matches is `…`, its number kept.
    assert_eq!(
        report.matches(&format!("…; count=4{at}")).count(),
        2,
        "{report}"
    );
    // The production call site is reduced like any other.
    assert!(
        report.contains("could not move the previous log aside; error=$REDACTED @ crates/reprise-core/src/run_log.rs:"),
        "{report}"
    );
    // A dependency's event is its level, target and `…` in both blocks.
    assert!(
        file_text.contains(&format!(" WARN {FOREIGN_TARGET}: …\n")),
        "{file_text}"
    );
    assert!(report.contains(" connection: …\n"), "{report}");
    // And no needle of any kind reached the report.
    assert!(!report.to_lowercase().contains(NEEDLE), "{report}");
    for leaked in ["/home", "/srv", "Music", "No such file"] {
        assert!(!report.contains(leaked), "{leaked:?} in {report}");
    }
}

#[test]
fn fb_19_each_event_is_one_line_of_the_log_and_the_report_embeds_them_unchanged() {
    let (_, file_text) = through_both_layers(log_all_kinds_of_event);
    let facts = DiagnosticFacts {
        log_tail: Some(file_text.clone()),
        ..DiagnosticFacts::default()
    };

    let report = render_report(&facts, &DiagnosticLog::default());

    assert_eq!(file_text.lines().count(), 5, "{file_text}");
    assert!(
        report.ends_with(&format!("log, this run\n{}", file_text.trim_end())),
        "{report}"
    );
}
