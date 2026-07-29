//! One rule per test, named for the behaviour it locks down — same shape
//! `reprise-runtime-client`'s own `mirror_tests.rs` uses.
//!
//! None of this needs a session bus. [`test_client`] does start a real
//! [`RuntimeClient`] (a `RuntimeSession` cannot exist without one — `client`
//! is not optional, matching production), but it points that client at a
//! bus name nobody owns and this file never even keeps the `RuntimeEvents`
//! half of the pair. `reprise_runtime_client::start`'s own doc comment is
//! explicit that constructing a client never fails and never blocks on a
//! bus being present — a missing or unreachable name just makes its worker
//! thread report `Disconnected` on a channel nothing here drains. Every
//! test below drives [`RuntimeSession::apply`] directly with synthetic
//! events instead, which is the same seam the real `glib` pump
//! (`session.rs`'s `spawn_pump`) calls — nothing about the fold, the
//! fan-out, or the accessors differs between a synthetic event and a real
//! one reaching that call. Tests needing the pump or an actual handshake
//! belong in a display-free integration test of their own, matching
//! `crates/reprise-platform-linux/tests/runtime_service.rs`'s `#[ignore]`d
//! session-bus tests — none of that is exercised here.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};

use reprise_runtime_protocol::runtime::RuntimeSnapshot;

use super::*;

fn test_client() -> RuntimeClient {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let bus_name = format!(
        "org.reprise.Reprise1.test.runtime_session.{}.{id}",
        std::process::id()
    );
    // `.0`: the `RuntimeEvents` half is dropped immediately. See the module
    // doc comment above for why that is safe here.
    reprise_runtime_client::start_with_bus_name(Vec::new(), bus_name).0
}

fn test_session() -> Rc<RuntimeSession> {
    RuntimeSession::from_client(test_client())
}

fn playback(status: &str) -> PlaybackSnapshot {
    PlaybackSnapshot {
        status: status.into(),
        ..Default::default()
    }
}

fn queue(current_track_id: i64) -> QueueSnapshot {
    QueueSnapshot {
        current_track_id: Some(current_track_id),
        ..Default::default()
    }
}

fn device_run(device: &str) -> DeviceRunSnapshot {
    DeviceRunSnapshot {
        device: device.into(),
        phase: "copying".into(),
        ..Default::default()
    }
}

fn job(job_id: i64) -> JobSnapshot {
    JobSnapshot {
        job_id,
        state: "running".into(),
        ..Default::default()
    }
}

fn snapshot(sequence: u64) -> RuntimeSnapshot {
    RuntimeSnapshot {
        protocol_major: 1,
        protocol_minor: 0,
        sequence,
        client_id: 1,
        playback: playback("playing"),
        queue: queue(1),
        device_runs: vec![device_run("Pixel 8")],
        jobs: vec![job(1)],
    }
}

fn connected(snapshot: RuntimeSnapshot) -> ClientEvent {
    ClientEvent::Connected(Box::new(snapshot))
}

/// Counts calls without needing `Rc<RefCell<_>>` boilerplate at every call
/// site — every test below that only cares "did this fire, how many times"
/// clones this into a `move` closure.
fn counter() -> (Rc<Cell<u32>>, impl Fn() + Clone) {
    let count = Rc::new(Cell::new(0));
    let recorder = {
        let count = Rc::clone(&count);
        move || count.set(count.get() + 1)
    };
    (count, recorder)
}

#[test]
fn a_session_with_no_connection_yet_reports_no_state() {
    let session = test_session();

    assert!(!session.is_connected());
    assert_eq!(session.playback(), None);
    assert_eq!(session.queue(), None);
    assert!(session.device_runs().is_empty());
    assert!(session.jobs().is_empty());
}

#[test]
fn connecting_reports_the_snapshot_and_notifies_state_changed() {
    let session = test_session();
    let (calls, record) = counter();
    session.add_on_state_changed(record);

    session.apply(&connected(snapshot(1)));

    assert_eq!(calls.get(), 1, "a first snapshot is a state change");
    assert!(session.is_connected());
    assert_eq!(session.playback(), Some(playback("playing")));
    assert_eq!(session.queue(), Some(queue(1)));
    assert_eq!(session.device_runs(), vec![device_run("Pixel 8")]);
    assert_eq!(session.jobs(), vec![job(1)]);
}

#[test]
fn disconnecting_after_a_connection_clears_everything_and_notifies_again() {
    let session = test_session();
    let (calls, record) = counter();
    session.add_on_state_changed(record);
    session.apply(&connected(snapshot(1)));

    session.apply(&ClientEvent::Disconnected);

    assert_eq!(
        calls.get(),
        2,
        "connect and disconnect are each their own state change"
    );
    assert!(!session.is_connected());
    assert_eq!(
        session.playback(),
        None,
        "RUN-2: unavailable, never the last known value"
    );
    assert_eq!(session.queue(), None);
    assert!(session.device_runs().is_empty());
    assert!(session.jobs().is_empty());
}

#[test]
fn a_refusal_clears_state_exactly_like_a_disconnection() {
    let session = test_session();
    session.apply(&connected(snapshot(1)));

    session.apply(&ClientEvent::Refused(ClientError::Refused(
        "refused:protocol_mismatch".into(),
    )));

    assert!(!session.is_connected());
    assert_eq!(session.playback(), None);
}

#[test]
fn a_reconnect_snapshot_replaces_state_rather_than_merging_it() {
    let session = test_session();
    session.apply(&connected(snapshot(1)));
    session.apply(&ClientEvent::Disconnected);

    let mut second = snapshot(1);
    second.playback = playback("paused");
    second.device_runs = Vec::new();
    session.apply(&connected(second));

    assert!(session.is_connected());
    assert_eq!(session.playback(), Some(playback("paused")));
    assert!(
        session.device_runs().is_empty(),
        "the new snapshot has no device runs; the old one must not survive it"
    );
}

#[test]
fn a_playback_delta_updates_the_mirror_and_notifies_subscribers() {
    let session = test_session();
    session.apply(&connected(snapshot(1)));
    let (calls, record) = counter();
    session.add_on_state_changed(record);

    session.apply(&ClientEvent::PlaybackChanged {
        sequence: 2,
        initiator: None,
        snapshot: playback("paused"),
    });

    assert_eq!(calls.get(), 1);
    assert_eq!(session.playback(), Some(playback("paused")));
    // The queue was untouched by this delta.
    assert_eq!(session.queue(), Some(queue(1)));
}

#[test]
fn a_stale_delta_is_dropped_and_reports_no_change() {
    let session = test_session();
    session.apply(&connected(snapshot(5)));
    let (calls, record) = counter();
    session.add_on_state_changed(record);

    // Sequence 3 is behind the snapshot's own sequence 5.
    session.apply(&ClientEvent::PlaybackChanged {
        sequence: 3,
        initiator: None,
        snapshot: playback("paused"),
    });

    assert_eq!(calls.get(), 0, "a stale delta changes nothing to notify");
    assert_eq!(session.playback(), Some(playback("playing")));
}

#[test]
fn a_command_failure_notifies_only_command_failed_subscribers() {
    let session = test_session();
    let (state_calls, record_state) = counter();
    session.add_on_state_changed(record_state);
    let seen: Rc<Cell<Option<ClientError>>> = Rc::new(Cell::new(None));
    let captured = Rc::clone(&seen);
    session.add_on_command_failed(move |_command, error| {
        captured.set(Some(error.clone()));
    });
    let command =
        RuntimeCommand::Playback(reprise_runtime_protocol::playback::PlaybackCommand::Play);
    let error = ClientError::Unavailable("unavailable:not_connected".into());

    session.apply(&ClientEvent::CommandFailed {
        request: reprise_runtime_client::RequestId::from(1),
        command,
        error: error.clone(),
    });

    assert_eq!(
        seen.take(),
        Some(error),
        "the command-failed subscriber saw the error"
    );
    assert_eq!(
        state_calls.get(),
        0,
        "a command's own failure carries no runtime state (see RuntimeMirror::apply)"
    );
}

#[test]
fn every_subscriber_is_called_in_subscription_order() {
    let session = test_session();
    let order = Rc::new(Cell::new(Vec::<u32>::new()));
    for id in 0..3u32 {
        let order = Rc::clone(&order);
        session.add_on_state_changed(move || {
            let mut seen = order.take();
            seen.push(id);
            order.set(seen);
        });
    }

    session.apply(&connected(snapshot(1)));

    assert_eq!(order.take(), vec![0, 1, 2]);
}

#[test]
fn a_state_changed_callback_may_subscribe_again_without_a_borrow_panic() {
    let session = test_session();
    let (extra_calls, record_extra) = counter();
    let record_extra = Rc::new(record_extra);
    {
        // A clone the outer closure captures, kept distinct from `session`
        // itself so calling `session.add_on_state_changed` below borrows
        // `session`, not the value the closure moves in.
        let inner_target = Rc::clone(&session);
        let record_extra = Rc::clone(&record_extra);
        session.add_on_state_changed(move || {
            // Registers a second subscriber from inside a callback that is
            // itself running as part of `notify_state_changed`'s fan-out.
            // If the callback list were still borrowed at this point this
            // would panic with `BorrowMutError` — see `session.rs`'s
            // `notify_state_changed` doc comment for why it clones the
            // list out first instead.
            let record_extra = Rc::clone(&record_extra);
            inner_target.add_on_state_changed(move || record_extra());
        });
    }

    session.apply(&connected(snapshot(1)));
    assert_eq!(
        extra_calls.get(),
        0,
        "not registered until the fold above ran"
    );

    session.apply(&ClientEvent::Disconnected);
    assert_eq!(
        extra_calls.get(),
        1,
        "the newly-registered subscriber fires on the next state change"
    );
}

#[test]
fn commands_may_be_sent_while_disconnected_without_panicking() {
    let session = test_session();

    // `RuntimeClient::send` never blocks and reports a failure as an event
    // rather than an error return (see `session.rs`'s `send` doc comment);
    // a disconnected client refuses the command internally instead of
    // panicking. This is a smoke test of the wiring, not the mapping itself
    // — `RuntimeCommand::wire` has its own coverage in
    // `reprise-runtime-client`.
    session.play();
    session.pause();
    session.next();
    session.previous();
    session.set_volume(0.5);
    session.seek(-1_000);
    session.play_tracks(vec![1, 2, 3], 0);
    session.queue_add_next(vec![4]);
    session.queue_clear();
    session.device_start("Pixel 8");
    session.job_cancel(1);
}
