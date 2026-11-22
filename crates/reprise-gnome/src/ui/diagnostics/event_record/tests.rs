use std::sync::{Arc, Mutex};

use reprise_core::diagnostics::{LogLevel, RecordedEvent, RecordedField, RecordedValue};
use tracing::{Event, Subscriber};
use tracing_subscriber::layer::{Context, Layer, SubscriberExt};

use super::{callsite_of, recorded_of};

struct Probe(Arc<Mutex<Vec<RecordedEvent>>>);

impl<S: Subscriber> Layer<S> for Probe {
    fn on_event(&self, event: &Event<'_>, _context: Context<'_, S>) {
        self.0.lock().unwrap().push(recorded_of(event));
    }
}

fn recorded(log: impl FnOnce()) -> RecordedEvent {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let subscriber = tracing_subscriber::registry().with(Probe(seen.clone()));
    tracing::subscriber::with_default(subscriber, log);
    let events = seen.lock().unwrap().clone();
    assert_eq!(events.len(), 1, "{events:?}");
    events[0].clone()
}

fn field(event: &RecordedEvent, key: &str) -> RecordedValue {
    let found = event.fields.iter().find(|field| field.key == key);
    let Some(RecordedField { value, .. }) = found else {
        panic!("no field {key:?} in {event:?}");
    };
    value.clone()
}

#[test]
fn each_field_is_recorded_with_the_kind_its_call_site_gave_it() {
    let error = std::io::Error::other("zqx failure");
    let event = recorded(|| {
        tracing::info!(
            signed = -5_i64,
            unsigned = 7_u64,
            wide_signed = -9_i128,
            wide_unsigned = 11_u128,
            ratio = 1.5_f64,
            flag = true,
            text = "plain",
            shown = %"displayed",
            listed = ?Some(3),
            failure = &error as &(dyn std::error::Error + 'static),
            "ordinary message"
        );
    });

    assert_eq!(field(&event, "signed"), RecordedValue::Int(-5));
    assert_eq!(field(&event, "unsigned"), RecordedValue::Uint(7));
    assert_eq!(field(&event, "wide_signed"), RecordedValue::Int(-9));
    assert_eq!(field(&event, "wide_unsigned"), RecordedValue::Uint(11));
    assert_eq!(field(&event, "ratio"), RecordedValue::Float(1.5));
    assert_eq!(field(&event, "flag"), RecordedValue::Bool(true));
    assert_eq!(field(&event, "text"), RecordedValue::Str("plain".into()));
    assert_eq!(
        field(&event, "shown"),
        RecordedValue::Debug("displayed".into())
    );
    assert_eq!(
        field(&event, "listed"),
        RecordedValue::Debug("Some(3)".into())
    );
    assert!(matches!(
        field(&event, "failure"),
        RecordedValue::Debug(text) if text.contains("zqx failure")
    ));
}

#[test]
fn the_message_field_fills_the_message_slot_and_no_field() {
    let event = recorded(|| tracing::warn!(count = 2, "stalled after {} tries", 3));

    assert_eq!(event.message.as_deref(), Some("stalled after 3 tries"));
    assert_eq!(event.fields.len(), 1, "{event:?}");
    assert_eq!(field(&event, "count"), RecordedValue::Int(2));
}

#[test]
fn an_event_without_a_message_has_none() {
    let event = recorded(|| tracing::warn!(count = 2));

    assert_eq!(event.message, None);
}

#[test]
fn the_callsite_carries_the_static_location_target_and_level() {
    let seen = Arc::new(Mutex::new(None));
    struct Callsites(Arc<Mutex<Option<reprise_core::diagnostics::Callsite>>>);
    impl<S: Subscriber> Layer<S> for Callsites {
        fn on_event(&self, event: &Event<'_>, _context: Context<'_, S>) {
            *self.0.lock().unwrap() = Some(callsite_of(event.metadata()));
        }
    }
    let subscriber = tracing_subscriber::registry().with(Callsites(seen.clone()));

    let line = line!() + 1;
    tracing::subscriber::with_default(subscriber, || tracing::error!(target: "probe", "x"));

    let callsite = seen.lock().unwrap().unwrap();
    assert_eq!(
        callsite.file,
        Some("crates/reprise-gnome/src/ui/diagnostics/event_record/tests.rs")
    );
    assert_eq!(callsite.line, Some(line));
    assert_eq!(callsite.target, "probe");
    assert_eq!(callsite.level, LogLevel::Error);
}
