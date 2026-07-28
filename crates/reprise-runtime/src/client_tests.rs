//! Fan-out, mailbox bounds and the identity of a client handle.

use reprise_runtime_protocol::playback::PlaybackSnapshot;

use super::{ClientHandshake, Clients};
use crate::error::Capability;
use crate::event::RuntimeEvent;

fn an_event(status: &str) -> RuntimeEvent {
    RuntimeEvent::PlaybackChanged(PlaybackSnapshot {
        status: status.into(),
        ..PlaybackSnapshot::default()
    })
}

#[test]
fn a_reconnect_gets_a_new_handle_rather_than_the_old_one() {
    let mut clients = Clients::new();
    let first = clients.connect(Default::default());
    clients.disconnect(first);
    let second = clients.connect(Default::default());

    assert_ne!(
        first, second,
        "reusing an id would let a command written for the old session bind \
         to the new one"
    );
    assert!(!clients.is_connected(first));
    assert!(clients.is_connected(second));
}

#[test]
fn disconnecting_twice_is_a_no_op_the_second_time() {
    let mut clients = Clients::new();
    let client = clients.connect(Default::default());

    assert!(clients.disconnect(client));
    assert!(!clients.disconnect(client));
}

#[test]
fn events_published_while_nobody_listens_still_advance_the_sequence() {
    let mut clients = Clients::new();
    clients.publish(an_event("playing"));
    let joined = clients.connect(Default::default());

    assert_eq!(
        clients.sequence(),
        1,
        "the order is the runtime's, not any client's"
    );
    assert!(
        clients.drain(joined).unwrap().events.is_empty(),
        "a client that was not there does not receive what it missed"
    );
}

#[test]
fn a_mailbox_that_overflows_drops_the_oldest_and_asks_for_a_resynchronization() {
    let mut clients = Clients::new();
    let client = clients.connect(Default::default());
    // One more than the mailbox holds.
    for step in 0..=super::MAILBOX_CAPACITY {
        clients.publish(an_event(&format!("step-{step}")));
    }

    let delivery = clients.drain(client).unwrap();

    assert_eq!(delivery.events.len(), super::MAILBOX_CAPACITY);
    assert!(delivery.resynchronize);
    assert_eq!(
        delivery.events[0].sequence, 2,
        "the oldest was dropped, so the surviving run starts one later — \
         which is exactly why the client must not apply it as a delta"
    );
    assert!(
        !clients.drain(client).unwrap().resynchronize,
        "the flag is reported once and then cleared, so a caught-up client \
         is not told to resynchronize forever"
    );
}

#[test]
fn a_capability_is_checked_against_the_holder_and_nobody_else() {
    let mut clients = Clients::new();
    let holder = clients.connect([Capability::DeviceSync].into_iter().collect());
    let other = clients.connect(Default::default());

    assert!(clients.holds(holder, Capability::DeviceSync));
    assert!(!clients.holds(holder, Capability::PlaybackControl));
    assert!(!clients.holds(other, Capability::DeviceSync));
}

#[test]
fn a_handshake_announces_the_version_this_build_speaks() {
    let handshake = ClientHandshake::new([Capability::PlaybackControl]);

    assert_eq!(
        handshake.protocol,
        reprise_runtime_protocol::PROTOCOL_VERSION
    );
}
