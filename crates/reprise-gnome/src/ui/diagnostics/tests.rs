use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};

use reprise_core::diagnostics::{
    templates_for_file, DiagnosticEvent, DiagnosticLevel, DiagnosticLog, LogTemplate,
};
use tracing::{Event, Subscriber};
use tracing_subscriber::layer::{Context, Layer, SubscriberExt};

use super::event_record::{callsite_of, recorded_of};
use super::run_log_fixture_tests::move_the_previous_log_aside_and_fail;
use super::{session_filter, SessionDiagnosticLayer};

fn session(log: impl FnOnce()) -> Vec<DiagnosticEvent> {
    session_with(|layer| layer, log)
}

fn session_with<L>(
    wrap: impl FnOnce(SessionDiagnosticLayer) -> L,
    log: impl FnOnce(),
) -> Vec<DiagnosticEvent>
where
    L: Layer<tracing_subscriber::Registry> + Send + Sync + 'static,
{
    let store = Arc::new(Mutex::new(DiagnosticLog::default()));
    let subscriber =
        tracing_subscriber::registry().with(wrap(SessionDiagnosticLayer::new(store.clone())));
    tracing::subscriber::with_default(subscriber, log);
    let events = store.lock().unwrap().events().cloned().collect();
    events
}

#[test]
fn session_layer_keeps_warn_and_error_but_not_info() {
    let events = session(|| {
        tracing::info!(target: "reprise_core::scanner", ignored = 1, "ordinary progress");
        tracing::warn!(target: "reprise_core::scanner", path = "/private/song.flac", "scan warning");
        tracing::error!(target: "reprise_core::scanner", reason = "decoder", "playback failed");
    });

    let levels: Vec<_> = events.iter().map(|event| event.level).collect();
    assert_eq!(levels, [DiagnosticLevel::Warn, DiagnosticLevel::Error]);
}

#[test]
fn an_event_from_outside_the_template_table_is_stored_as_an_ellipsis_without_fields() {
    let events = session(|| {
        tracing::warn!(
            target: "reprise_core::scanner",
            device_id = "0123456789ABCDEF",
            playlist = "Zqx Roadtrip",
            track_id = 42,
            "device sync failed for zqxlistener"
        );
    });

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].message, "…");
    assert_eq!(events[0].target, "reprise_core::scanner");
    assert_eq!(events[0].location, None);
}

#[test]
fn an_own_event_is_stored_as_its_static_template_its_fields_and_its_call_site() {
    let events = session(move_the_previous_log_aside_and_fail);

    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(
        events[0].message,
        "could not move the previous log aside; error=$REDACTED"
    );
    assert_eq!(events[0].level, DiagnosticLevel::Warn);
    let location = events[0].location.as_deref().unwrap();
    assert!(
        location.starts_with("crates/reprise-core/src/run_log.rs:"),
        "{location}"
    );
}

#[test]
fn session_filter_excludes_events_from_external_crates() {
    let events = session_with(
        |layer| layer.with_filter(session_filter()),
        || {
            tracing::warn!(target: "foreign_transport::connection", "foreign transport warning");
            tracing::warn!(target: "reprise_core::scanner", "owned scanner warning");
        },
    );

    let targets: Vec<_> = events.iter().map(|event| event.target.as_str()).collect();
    assert_eq!(targets, ["reprise_core::scanner"]);
}

/// What a probe saw of one event: its call site's file and line, and its message.
type Seen = (Option<&'static str>, Option<u32>, Option<String>);

/// Trips an invariant under a probe: the line the macro is written on, and what the probe saw.
fn trip_an_invariant() -> (u32, Vec<Seen>) {
    struct Probe(Arc<Mutex<Vec<Seen>>>);
    impl<S: Subscriber> Layer<S> for Probe {
        fn on_event(&self, event: &Event<'_>, _context: Context<'_, S>) {
            let callsite = callsite_of(event.metadata());
            self.0
                .lock()
                .unwrap()
                .push((callsite.file, callsite.line, recorded_of(event).message));
        }
    }
    let seen = Arc::new(Mutex::new(Vec::new()));
    let subscriber = tracing_subscriber::registry().with(Probe(seen.clone()));
    let line = line!() + 3;
    tracing::subscriber::with_default(subscriber, || {
        // The debug assertion fires after the log call; release builds skip it.
        let _ = std::panic::catch_unwind(|| invariant_broken!("row {} has no model", 7));
    });
    let events = seen.lock().unwrap().clone();
    (line, events)
}

// `file!()` in a `macro_rules!` expansion is the outermost invocation, so an
// invariant's template is filed (by the generator) and matched (at run time)
// under the file that uses the macro, not under `invariant.rs`, and its text is
// the caller's literal behind `invariant broken: ` with no location suffix.
#[test]
fn an_invariants_template_is_filed_and_matched_at_the_use_site() {
    let (line, events) = trip_an_invariant();

    assert_eq!(events.len(), 1, "{events:?}");
    let (file, logged_line, message) = &events[0];
    assert_eq!(*file, Some(file!()));
    assert_ne!(*file, Some("crates/reprise-gnome/src/ui/invariant.rs"));
    assert_eq!(*logged_line, Some(line));
    let message = message.as_deref().unwrap();
    assert_eq!(message, "invariant broken: row 7 has no model");
    let template = LogTemplate::parse("invariant broken: row {} has no model").unwrap();
    assert!(template.matches(message), "{message}");
}

// The template table is keyed by `file!()` as written from the workspace root
// (`reprise-core` pins the reasoning). A build that changes the form would turn
// every message of this crate into `…` in the report, so the form is pinned.
#[test]
fn a_call_sites_file_has_the_template_tables_path_form() {
    assert_eq!(file!(), "crates/reprise-gnome/src/ui/diagnostics/tests.rs");
}

// An invariant whose literal is only `{}: {}` would reach the report as `…: …`.
// The words that say what broke are written inline at the call site, so the
// checked-in table carries them.
#[test]
fn an_invariant_with_a_constant_part_files_its_words_in_the_table() {
    let canonical = |file| {
        templates_for_file(file)
            .iter()
            .map(LogTemplate::canonical)
            .collect::<Vec<_>>()
    };

    assert!(
        canonical("crates/reprise-gnome/src/ui/device_sync/device_sync_effects.rs").contains(
            &"invariant broken: the sync machine emitted an invalid transfer index: {}".to_owned()
        )
    );
    assert!(
        canonical("crates/reprise-gnome/src/ui/table_columns/registry.rs")
            .contains(&"invariant broken: invalid column binding: {}".to_owned())
    );
}

// The seconds are the UTC ones whatever zone the machine is in, so the report
// carries no UTC offset. The function takes a `DateTime<Utc>`: a local time
// cannot be passed to it.
#[test]
fn the_last_warnings_seconds_are_those_of_the_utc_day() {
    let at = DateTime::parse_from_rfc3339("2026-11-23T23:59:58+02:00")
        .unwrap()
        .with_timezone(&Utc);

    assert_eq!(
        super::seconds_since_midnight_utc(at),
        21 * 3600 + 59 * 60 + 58
    );
}
