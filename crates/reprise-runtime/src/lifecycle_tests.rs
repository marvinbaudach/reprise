//! Every transition in §9.2's diagram, and the ones it deliberately omits.

use std::time::Duration;

use super::{Lifecycle, LifecycleChange, LifecycleMachine, RefusalCause};

const GRACE_MS: u64 = 1_000;

fn machine() -> LifecycleMachine {
    LifecycleMachine::with_grace(Duration::from_millis(GRACE_MS))
}

#[test]
fn a_starting_runtime_does_not_drain_before_it_has_served_anything() {
    let mut machine = machine();

    assert_eq!(machine.observe(true, 0), None);
    assert_eq!(machine.observe(true, GRACE_MS * 10), None);
    assert_eq!(
        machine.state(),
        Lifecycle::Starting,
        "a runtime that exited between activation and its first command \
         would make the client that woke it look like it failed"
    );
}

#[test]
fn an_idle_serving_runtime_starts_the_grace_and_ends_after_it() {
    let mut machine = machine();
    machine.serve();

    assert_eq!(
        machine.observe(true, 100),
        Some(LifecycleChange::EnteredDraining)
    );
    assert_eq!(machine.state(), Lifecycle::Draining { since_ms: 100 });
    assert_eq!(
        machine.observe(true, 100 + GRACE_MS - 1),
        None,
        "one millisecond short of the grace is still inside it"
    );
    assert_eq!(
        machine.observe(true, 100 + GRACE_MS),
        Some(LifecycleChange::Shutdown)
    );
    assert_eq!(machine.state(), Lifecycle::Stopping);
    assert!(!machine.is_running());
}

#[test]
fn run_4_a_busy_runtime_never_starts_the_grace_at_all() {
    let mut machine = machine();
    machine.serve();

    for tick in 0..10 {
        assert_eq!(machine.observe(false, tick * GRACE_MS), None);
    }
    assert_eq!(
        machine.state(),
        Lifecycle::Serving,
        "a service that abandons work to save memory is a data-loss feature"
    );
}

#[test]
fn anything_happening_during_the_grace_abandons_it() {
    let mut machine = machine();
    machine.serve();
    machine.observe(true, 0);

    assert_eq!(
        machine.observe(false, GRACE_MS / 2),
        Some(LifecycleChange::LeftDraining)
    );
    assert_eq!(machine.state(), Lifecycle::Serving);
}

#[test]
fn a_second_grace_is_measured_from_its_own_start_not_the_abandoned_one() {
    let mut machine = machine();
    machine.serve();
    machine.observe(true, 0);
    machine.observe(false, GRACE_MS - 1);

    machine.observe(true, GRACE_MS);
    assert_eq!(
        machine.observe(true, GRACE_MS + GRACE_MS - 1),
        None,
        "carrying the first attempt's elapsed time over would shut the \
         runtime down almost immediately after a client left again"
    );
    assert_eq!(
        machine.observe(true, GRACE_MS * 2),
        Some(LifecycleChange::Shutdown)
    );
}

#[test]
fn a_client_connecting_during_the_grace_returns_to_serving() {
    let mut machine = machine();
    machine.serve();
    machine.observe(true, 0);

    machine.serve();

    assert_eq!(
        machine.state(),
        Lifecycle::Serving,
        "connecting is one of the things that aborts draining, and it must \
         not have to wait for the next observation to do so"
    );
}

#[test]
fn a_refused_start_is_terminal() {
    let mut machine = machine();

    machine.refuse(RefusalCause::LeaseHeld);

    assert_eq!(machine.state(), Lifecycle::Refused(RefusalCause::LeaseHeld));
    assert!(!machine.is_running());
    machine.serve();
    assert_eq!(
        machine.state(),
        Lifecycle::Refused(RefusalCause::LeaseHeld),
        "a refused process does not wait and does not restart itself"
    );
    assert_eq!(machine.observe(false, GRACE_MS), None);
}

#[test]
fn a_serving_runtime_cannot_be_refused_after_the_fact() {
    let mut machine = machine();
    machine.serve();

    machine.refuse(RefusalCause::ProtocolMajor);

    assert_eq!(
        machine.state(),
        Lifecycle::Serving,
        "one incompatible client is refused; the runtime serving the others \
         is not"
    );
}

#[test]
fn a_stopped_runtime_does_not_come_back() {
    let mut machine = machine();
    machine.serve();
    machine.observe(true, 0);
    machine.observe(true, GRACE_MS);

    machine.serve();

    assert_eq!(machine.state(), Lifecycle::Stopping);
    assert_eq!(machine.observe(false, GRACE_MS * 2), None);
}
