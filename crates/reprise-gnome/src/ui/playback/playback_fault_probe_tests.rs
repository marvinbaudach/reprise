//! A faulted track's file is probed off the main thread: the skip is
//! immediate, the toast follows the probe's answer, and a probe that does not
//! answer in time never freezes the window or doubles the toast.

use std::cell::Cell;
use std::path::Path;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use gtk4::glib;

use super::fault_probe_slots::{MountKey, MAX_FAULT_PROBES_PER_MOUNT};
use super::playback_fault_probe::{FaultProbe, FaultProbeAnswer};
use super::playback_fault_progress_tests::{
    playing_repeat_all_queue, stalled_start, Fixture, TRACKS,
};
use super::playback_faults::{recorded_fault_toasts, FAULT_PROBE_WAIT};
use crate::ui::strings;

/// The longest the main context may go without running a heartbeat before the
/// window counts as frozen (the plan's bound).
const FROZEN_AFTER: Duration = Duration::from_millis(500);
const HEARTBEAT: Duration = Duration::from_millis(50);
/// How long a probe waits for its release before giving up. Bounded so that a
/// handler which calls the probe inline fails the heartbeat check instead of
/// deadlocking the test.
const PROBE_GATE_LIMIT: Duration = Duration::from_secs(3);
/// Long enough for a quick probe's answer to arrive and for a wrongly doubled
/// toast to show up behind it.
const SETTLE: Duration = Duration::from_millis(400);
const FAULTED: i64 = 41;

fn spin(duration: Duration) {
    let context = glib::MainContext::default();
    let until = Instant::now() + duration;
    while Instant::now() < until {
        context.iteration(false);
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn answering(answer: FaultProbeAnswer, calls: &Arc<AtomicUsize>) -> FaultProbe {
    let calls = calls.clone();
    Arc::new(move |_| {
        calls.fetch_add(1, Ordering::SeqCst);
        answer
    })
}

fn count_reloads(fixture: &Fixture) -> Rc<Cell<usize>> {
    let reloads = Rc::new(Cell::new(0));
    let counter = reloads.clone();
    fixture
        .controller
        .set_track_list_reload(move || counter.set(counter.get() + 1));
    reloads
}

fn missing_since_is_set(fixture: &Fixture, id: i64) -> bool {
    let connection = crate::test_db::connection(&fixture.controller.conn);
    connection
        .query_row(
            "SELECT missing_since IS NOT NULL FROM tracks WHERE id = ?1",
            [id],
            |row| row.get(0),
        )
        .unwrap()
}

fn path_of(fixture: &Fixture, id: i64) -> String {
    reprise_core::queries::query_track_summary(&fixture.controller.conn, id)
        .unwrap()
        .unwrap()
        .path
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn a_probe_that_blocks_neither_freezes_the_window_nor_delays_the_skip() {
    let fixture = playing_repeat_all_queue();
    let controller = &fixture.controller;
    let reloads = count_reloads(&fixture);
    let (release, gate) = mpsc::channel::<()>();
    let gate = Mutex::new(gate);
    *controller.fault_probe.borrow_mut() = Arc::new(move |_| {
        let _ = gate.lock().unwrap().recv_timeout(PROBE_GATE_LIMIT);
        FaultProbeAnswer::FileMissing { marked: true }
    });
    let last_beat = Rc::new(Cell::new(Instant::now()));
    let longest_gap = Rc::new(Cell::new(Duration::ZERO));
    let heartbeat = glib::timeout_add_local(HEARTBEAT, {
        let (last_beat, longest_gap) = (last_beat.clone(), longest_gap.clone());
        move || {
            let now = Instant::now();
            longest_gap.set(longest_gap.get().max(now - last_beat.get()));
            last_beat.set(now);
            glib::ControlFlow::Continue
        }
    });
    recorded_fault_toasts();

    let started = Instant::now();
    controller.handle_unplayable_track(FAULTED);

    assert!(
        started.elapsed() < FROZEN_AFTER,
        "the handler returned only after the probe: {:?}",
        started.elapsed()
    );
    assert_eq!(
        fixture.calls.played.borrow().len(),
        2,
        "the skip does not wait for the probe"
    );
    assert!(
        recorded_fault_toasts().is_empty(),
        "no toast before the wait"
    );

    spin(FAULT_PROBE_WAIT + SETTLE);
    assert_eq!(
        recorded_fault_toasts(),
        vec![strings::could_not_play_toast("Track 41")],
        "one neutral toast once the probe is overdue"
    );

    release.send(()).unwrap();
    spin(SETTLE);
    heartbeat.remove();

    assert!(
        recorded_fault_toasts().is_empty(),
        "the late answer adds no second toast"
    );
    assert_eq!(reloads.get(), 1, "the late answer still reloads the list");
    assert!(
        longest_gap.get() < FROZEN_AFTER,
        "the main context stalled for {:?}",
        longest_gap.get()
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn a_missing_file_is_marked_and_skipped_with_one_unavailable_toast() {
    let fixture = playing_repeat_all_queue();
    let controller = &fixture.controller;
    let reloads = count_reloads(&fixture);
    std::fs::remove_file(path_of(&fixture, FAULTED)).unwrap();
    recorded_fault_toasts();

    controller.handle_unplayable_track(FAULTED);
    assert_eq!(fixture.calls.played.borrow().len(), 2, "skipped at once");
    spin(SETTLE);

    assert!(missing_since_is_set(&fixture, FAULTED));
    assert_eq!(reloads.get(), 1);
    assert_eq!(
        recorded_fault_toasts(),
        vec![strings::text(strings::TRACK_UNAVAILABLE_SKIPPED)]
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn a_file_that_is_still_there_is_skipped_unmarked_with_one_could_not_play_toast() {
    let fixture = playing_repeat_all_queue();
    let controller = &fixture.controller;
    let reloads = count_reloads(&fixture);
    recorded_fault_toasts();

    controller.handle_unplayable_track(FAULTED);
    assert_eq!(fixture.calls.played.borrow().len(), 2, "skipped at once");
    spin(SETTLE);

    assert!(!missing_since_is_set(&fixture, FAULTED));
    assert_eq!(reloads.get(), 0);
    assert_eq!(
        recorded_fault_toasts(),
        vec![strings::could_not_play_toast("Track 41")]
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn a_track_whose_row_is_gone_is_skipped_without_a_probe_or_a_toast() {
    let fixture = playing_repeat_all_queue();
    let controller = &fixture.controller;
    let probes = Arc::new(AtomicUsize::new(0));
    *controller.fault_probe.borrow_mut() = answering(FaultProbeAnswer::FilePresent, &probes);
    crate::test_db::connection(&controller.conn)
        .execute("DELETE FROM tracks WHERE id = ?1", [FAULTED])
        .unwrap();
    recorded_fault_toasts();

    controller.handle_unplayable_track(FAULTED);
    spin(SETTLE);

    assert_eq!(fixture.calls.played.borrow().len(), 2, "skipped");
    assert_eq!(probes.load(Ordering::SeqCst), 0);
    assert!(recorded_fault_toasts().is_empty());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn the_skip_limit_still_stops_with_a_single_too_many_unplayable_toast() {
    let fixture = playing_repeat_all_queue();
    let controller = &fixture.controller;
    let probes = Arc::new(AtomicUsize::new(0));
    *controller.fault_probe.borrow_mut() = answering(FaultProbeAnswer::FilePresent, &probes);
    recorded_fault_toasts();

    for _ in 0..TRACKS.len() * 2 {
        controller.apply_event(stalled_start());
    }
    // The run's faults happen in one call stack, so the probes past the bound
    // never start and their toasts come from the overdue timer.
    spin(FAULT_PROBE_WAIT + SETTLE);

    let toasts = recorded_fault_toasts();
    let stopped = strings::text(strings::PLAYBACK_STOPPED_TOO_MANY_UNPLAYABLE);
    assert_eq!(toasts.iter().filter(|toast| **toast == stopped).count(), 1);
    assert_eq!(
        toasts.len(),
        1 + TRACKS.len(),
        "one could-not-play toast per fault, plus the stop: {toasts:?}"
    );
    assert_eq!(
        toasts.last(),
        Some(&stopped),
        "the stop comes after every fault toast: {toasts:?}"
    );
    assert_eq!(
        fixture.calls.played.borrow().len(),
        TRACKS.len(),
        "each track tried once"
    );
    assert_eq!(controller.current_track.get(), None);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn the_stop_toast_waits_for_the_fault_toasts_still_pending() {
    let fixture = playing_repeat_all_queue();
    let controller = &fixture.controller;
    let open = Arc::new(AtomicBool::new(false));
    *controller.fault_probe.borrow_mut() = Arc::new({
        let open = open.clone();
        move |_| {
            let give_up = Instant::now() + PROBE_GATE_LIMIT;
            while !open.load(Ordering::SeqCst) && Instant::now() < give_up {
                std::thread::sleep(HEARTBEAT);
            }
            FaultProbeAnswer::FilePresent
        }
    });
    recorded_fault_toasts();

    for _ in 0..TRACKS.len() * 2 {
        controller.apply_event(stalled_start());
    }
    assert_eq!(controller.current_track.get(), None, "the run has stopped");
    assert!(
        recorded_fault_toasts().is_empty(),
        "the stop toast does not overtake the fault toasts still pending"
    );

    spin(FAULT_PROBE_WAIT + SETTLE);
    open.store(true, Ordering::SeqCst);
    spin(SETTLE);

    let neutral = strings::could_not_play_toast;
    let stopped = strings::text(strings::PLAYBACK_STOPPED_TOO_MANY_UNPLAYABLE);
    let mut expected: Vec<String> = TRACKS
        .iter()
        .map(|id| neutral(&format!("Track {id}")))
        .collect();
    expected.push(stopped);
    assert_eq!(
        recorded_fault_toasts(),
        expected,
        "every fault toast, then the single stop"
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn the_skipped_episodes_summary_waits_for_the_fault_toasts_still_pending() {
    let fixture = playing_repeat_all_queue();
    let controller = &fixture.controller;
    let open = Arc::new(AtomicBool::new(false));
    *controller.fault_probe.borrow_mut() = Arc::new({
        let open = open.clone();
        move |_| {
            let give_up = Instant::now() + PROBE_GATE_LIMIT;
            while !open.load(Ordering::SeqCst) && Instant::now() < give_up {
                std::thread::sleep(HEARTBEAT);
            }
            FaultProbeAnswer::FilePresent
        }
    });
    recorded_fault_toasts();

    controller.handle_unplayable_track(FAULTED);
    controller.consecutive_episode_skips.set(2);
    controller.flush_episode_skip_toast();
    assert!(
        recorded_fault_toasts().is_empty(),
        "the summary does not overtake the fault toast still pending"
    );

    spin(FAULT_PROBE_WAIT + SETTLE);
    open.store(true, Ordering::SeqCst);
    spin(SETTLE);

    assert_eq!(
        recorded_fault_toasts(),
        vec![
            strings::could_not_play_toast("Track 41"),
            strings::skipped_unplayable_episodes(2),
        ],
        "the fault toast, then the summary"
    );
}

/// A probe that counts its calls and blocks until `open` is set, bounded by
/// [`PROBE_GATE_LIMIT`] so a test that never opens it still ends.
fn gated_probe(open: &Arc<AtomicBool>, calls: &Arc<AtomicUsize>) -> FaultProbe {
    let (open, calls) = (open.clone(), calls.clone());
    Arc::new(move |_| {
        calls.fetch_add(1, Ordering::SeqCst);
        let give_up = Instant::now() + PROBE_GATE_LIMIT;
        while !open.load(Ordering::SeqCst) && Instant::now() < give_up {
            std::thread::sleep(HEARTBEAT);
        }
        FaultProbeAnswer::FilePresent
    })
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn probes_in_flight_are_bounded_and_every_fault_still_gets_one_toast() {
    let fixture = playing_repeat_all_queue();
    let controller = &fixture.controller;
    let (open, probes) = (
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicUsize::new(0)),
    );
    *controller.fault_probe.borrow_mut() = gated_probe(&open, &probes);
    recorded_fault_toasts();

    assert!(TRACKS.len() > MAX_FAULT_PROBES_PER_MOUNT);
    for id in TRACKS {
        controller.handle_unplayable_track(id);
    }
    spin(SETTLE);
    assert_eq!(
        probes.load(Ordering::SeqCst),
        MAX_FAULT_PROBES_PER_MOUNT,
        "no probe past the bound while the others are blocked"
    );

    spin(FAULT_PROBE_WAIT);
    let neutral = strings::could_not_play_toast;
    let mut expected: Vec<String> = TRACKS
        .iter()
        .map(|id| neutral(&format!("Track {id}")))
        .collect();
    let toasts = recorded_fault_toasts();
    let stopped = strings::text(strings::PLAYBACK_STOPPED_TOO_MANY_UNPLAYABLE);
    if toasts.last() == Some(&stopped) {
        expected.push(stopped);
    }
    assert_eq!(
        toasts, expected,
        "one neutral toast per fault, the stop (if any) last"
    );

    open.store(true, Ordering::SeqCst);
    spin(SETTLE);
    assert!(
        recorded_fault_toasts().is_empty(),
        "the late answers add no toast"
    );

    controller.handle_unplayable_track(TRACKS[0]);
    spin(SETTLE);
    assert_eq!(
        probes.load(Ordering::SeqCst),
        MAX_FAULT_PROBES_PER_MOUNT + 1,
        "a released slot probes again"
    );
    let fault_toasts: Vec<String> = recorded_fault_toasts()
        .into_iter()
        .filter(|toast| *toast != strings::text(strings::PLAYBACK_STOPPED_TOO_MANY_UNPLAYABLE))
        .collect();
    assert_eq!(
        fault_toasts,
        vec![neutral("Track 41")],
        "and its answer shows the one toast"
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn a_track_already_being_probed_starts_no_second_probe() {
    let fixture = playing_repeat_all_queue();
    let controller = &fixture.controller;
    let (open, probes) = (
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicUsize::new(0)),
    );
    *controller.fault_probe.borrow_mut() = gated_probe(&open, &probes);
    recorded_fault_toasts();

    controller.handle_unplayable_track(FAULTED);
    controller.handle_unplayable_track(FAULTED);
    spin(SETTLE);
    assert_eq!(probes.load(Ordering::SeqCst), 1);

    spin(FAULT_PROBE_WAIT);
    assert_eq!(
        recorded_fault_toasts(),
        vec![strings::could_not_play_toast("Track 41"); 2],
        "each fault still gets its one neutral toast"
    );
}

const DEAD_TRACKS: [i64; 2] = [41, 42];
const LOCAL_TRACK: i64 = 43;

fn on_dead_mount(path: &Path) -> bool {
    DEAD_TRACKS
        .iter()
        .any(|id| path.ends_with(format!("track-{id}.flac")))
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn a_missing_file_on_a_healthy_disk_is_marked_while_a_dead_mount_holds_its_probe_slots() {
    let fixture = playing_repeat_all_queue();
    let controller = &fixture.controller;
    let reloads = count_reloads(&fixture);
    *controller.mount_resolver.borrow_mut() = Rc::new(|path| {
        MountKey::Mount(if on_dead_mount(path) { "dead" } else { "local" }.to_owned())
    });
    let (open, dead_probes) = (
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicUsize::new(0)),
    );
    let blocked = gated_probe(&open, &dead_probes);
    let healthy = super::playback_fault_probe::disk_probe();
    *controller.fault_probe.borrow_mut() = Arc::new(move |request| {
        if on_dead_mount(&request.path) {
            blocked(request)
        } else {
            healthy(request)
        }
    });
    assert_eq!(DEAD_TRACKS.len(), MAX_FAULT_PROBES_PER_MOUNT);
    std::fs::remove_file(path_of(&fixture, LOCAL_TRACK)).unwrap();
    recorded_fault_toasts();

    for id in DEAD_TRACKS {
        controller.handle_unplayable_track(id);
    }
    controller.handle_unplayable_track(LOCAL_TRACK);
    spin(SETTLE);

    assert_eq!(
        dead_probes.load(Ordering::SeqCst),
        MAX_FAULT_PROBES_PER_MOUNT,
        "both dead-mount probes are stuck"
    );
    assert!(
        missing_since_is_set(&fixture, LOCAL_TRACK),
        "the healthy disk still gets its probe"
    );
    assert_eq!(reloads.get(), 1);
    assert_eq!(
        recorded_fault_toasts(),
        vec![strings::text(strings::TRACK_UNAVAILABLE_SKIPPED)],
        "the healthy disk's answer shows its toast while the dead mount's are pending"
    );
    assert!(
        !missing_since_is_set(&fixture, DEAD_TRACKS[0]),
        "a probe that never answers marks nothing"
    );

    // Let the stuck probes end so no worker outlives the test.
    open.store(true, Ordering::SeqCst);
    spin(FAULT_PROBE_WAIT + SETTLE);
}
