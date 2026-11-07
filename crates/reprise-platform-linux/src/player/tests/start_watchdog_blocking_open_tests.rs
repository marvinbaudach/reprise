//! Proves the start a watchdog restart begins does not block the main context
//! when it is the new pipeline's own `open()` that blocks (#1323).
//!
//! The first start runs on a FIFO with a writer, so its open succeeds and it
//! hangs the way `start_watchdog_freeze_tests` has it. Just before the restart,
//! the path is made a new FIFO that has no writer: the new pipeline's open of it
//! blocks, and with it `set_state(PAUSED)` of that pipeline, for as long as no
//! writer shows up. A start over that waited for the state change on the main
//! context, under the playbin lock, would freeze both until then.
//!
//! The FIFO is released by the test, or by the clock if the test is the thread
//! that is stuck, so a regression fails on the gap instead of hanging.

use std::os::unix::fs::OpenOptionsExt;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{channel, Sender};
use std::time::Instant;

use super::start_watchdog_freeze_tests::{
    longest_gap, StuckFifo, HEARTBEAT, MAX_GAP, STUCK_FOR, WATCH_AFTER_RESTART,
};
use super::start_watchdog_restart_tests::restart_after;
use super::start_watchdog_tests::{harness_with_deadline, state_of, PATIENCE};
use super::*;
use crate::player::reaper;
use crate::player::start_watchdog::is_hung;

/// Roomy for the first open to have happened before the look finds the start
/// hung, short enough to keep the test brief.
const DEADLINE: Duration = Duration::from_millis(400);

/// How many descriptors of this process point at `path`.
fn open_descriptors_of(path: &std::path::Path) -> usize {
    std::fs::read_dir("/proc/self/fd")
        .map(|fds| {
            fds.filter_map(Result::ok)
                .filter_map(|fd| std::fs::read_link(fd.path()).ok())
                .filter(|target| target == path)
                .count()
        })
        .unwrap_or(0)
}

fn make_fifo(path: &std::path::Path) {
    let made = std::process::Command::new("mkfifo")
        .arg(path)
        .status()
        .unwrap();
    assert!(made.success(), "mkfifo failed");
}

/// Wakes whatever is blocked in `open()` of the FIFO at `path`: opens it for
/// reading and writing (which never blocks and counts as a writer), takes the
/// name away so no later open blocks, and closes it, which is end of file for
/// the reader that woke.
fn release_openers(path: &std::path::Path) {
    let writer = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_CLOEXEC)
        .open(path);
    let _ = std::fs::remove_file(path);
    drop(writer);
}

/// Releases the blocked openers at `path` when told to or after `STUCK_FOR`.
struct OpenerRelease {
    release: Option<Sender<()>>,
    releaser: Option<std::thread::JoinHandle<()>>,
}

impl OpenerRelease {
    fn at(path: &std::path::Path) -> Self {
        let (release, released) = channel::<()>();
        let path = path.to_owned();
        let releaser = std::thread::spawn(move || {
            let _ = released.recv_timeout(STUCK_FOR);
            release_openers(&path);
        });
        Self {
            release: Some(release),
            releaser: Some(releaser),
        }
    }
}

impl Drop for OpenerRelease {
    fn drop(&mut self) {
        if let Some(release) = self.release.take() {
            let _ = release.send(());
        }
        if let Some(releaser) = self.releaser.take() {
            let _ = releaser.join();
        }
    }
}

#[test]
fn a_restart_whose_new_pipeline_blocks_in_open_does_not_freeze_the_main_context() {
    let mut harness = harness_with_deadline(DEADLINE);
    let dir = tempfile::tempdir().unwrap();
    let fifo = dir.path().join("stuck.wav");
    // Declared after the harness, so both are released before it stops a
    // pipeline whose state change may still be blocked in `open()`.
    // Its own clock is the longer one: it takes the name away when it fires,
    // and the opener's release below needs the name to still be there.
    let stuck = StuckFifo::open_for(&fifo, STUCK_FOR * 3);
    let openers = OpenerRelease::at(&fifo);
    let swapped = Arc::new(AtomicBool::new(false));
    let (path, swapped_flag) = (fifo.clone(), swapped.clone());
    restart_after(&mut harness.player, move || {
        // Only once the first start has opened it (the test's own descriptor
        // and the source's): a start still on its way would block on the new one.
        if open_descriptors_of(&path) >= 2 {
            let _ = std::fs::remove_file(&path);
            make_fifo(&path);
            swapped_flag.store(true, Ordering::SeqCst);
        }
    });

    harness.player.play(item(fifo.to_str().unwrap())).unwrap();
    let beats = Arc::new(Mutex::new(vec![Instant::now()]));
    let recorded = beats.clone();
    let heartbeat = gst::glib::timeout_add(HEARTBEAT, move || {
        recorded
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(Instant::now());
        gst::glib::ControlFlow::Continue
    });
    let began = Instant::now();
    harness.pump_until(PATIENCE, |_| {
        harness.player.start_watchdog.restarts() >= 1 || began.elapsed() > STUCK_FOR * 2
    });
    harness.pump_for(WATCH_AFTER_RESTART);
    heartbeat.remove();

    let gap = longest_gap(&beats.lock().unwrap_or_else(PoisonError::into_inner));
    let (current, pending) = state_of(&harness.player);
    let restarts = harness.player.start_watchdog.restarts();
    assert!(
        swapped.load(Ordering::SeqCst),
        "the first start had not opened the FIFO before the restart: nothing was measured"
    );
    assert!(
        gap < MAX_GAP,
        "the main context stood still for {gap:?} while the restart's own start blocked in open()"
    );
    assert!(
        restarts >= 1,
        "the watchdog never restarted the stuck start"
    );
    assert!(
        is_hung(current, pending),
        "the new pipeline is not blocked in its start ({current:?} -> {pending:?}), so the \
         scenario did not block anything"
    );

    // Everything is released: the writer that wakes the blocked open, then the
    // first FIFO's, and every pipeline the reaper holds goes down.
    drop(openers);
    drop(stuck);
    let reaped = Instant::now();
    while reaper::in_flight::count() > 0 && reaped.elapsed() < PATIENCE {
        std::thread::sleep(HEARTBEAT);
    }
    assert_eq!(
        reaper::in_flight::count(),
        0,
        "a retired pipeline was never taken down"
    );
}
