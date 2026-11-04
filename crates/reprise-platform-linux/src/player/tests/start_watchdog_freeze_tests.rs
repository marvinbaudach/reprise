//! Proves a watchdog restart does not freeze the main context when the stuck
//! start sits inside a kernel read (#1323).
//!
//! The stuck source is a FIFO whose only writer is the test: a start of it
//! opens the file, reads what was written (a WAV header, no sample), and then
//! blocks in `read()` with nothing more to come, which `Null` cannot interrupt.
//! That is the shape a dead NFS/SMB mount gives a `filesrc`. The test's fd is
//! closed after a bounded time, so a regression stalls the main context for
//! that long and then fails on the gap, instead of hanging the suite.

use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::sync::mpsc::{channel, Sender};
use std::sync::Mutex;
use std::time::Instant;

use super::segment_support::Harness;
use super::*;
use crate::player::reaper;

/// The watchdog's deadline: short, so the restart lands within a second.
const TEST_DEADLINE: Duration = Duration::from_millis(150);
/// Far more than the test needs, and far less than a hang would take.
const PATIENCE: Duration = Duration::from_secs(20);
/// How often the heartbeat timer asks the main context to dispatch it.
const HEARTBEAT: Duration = Duration::from_millis(20);
/// The longest the main context may go without dispatching the heartbeat.
const MAX_GAP: Duration = Duration::from_millis(500);
/// How long the FIFO's writer stays open, whatever the test does: a regression
/// freezes the main context for about this long, and no longer.
const STUCK_FOR: Duration = Duration::from_secs(3);
/// How long the heartbeat is watched once the restart has happened.
const WATCH_AFTER_RESTART: Duration = Duration::from_millis(700);
/// The sample rate and size the WAV header declares; no sample ever follows.
const WAV_HEADER_LEN: u32 = 44;
const WAV_DECLARED_DATA: u32 = 1 << 24;

/// A 16-bit mono WAV header that promises far more data than is ever written.
fn wav_header() -> Vec<u8> {
    let mut header = Vec::with_capacity(WAV_HEADER_LEN as usize);
    header.extend_from_slice(b"RIFF");
    header.extend_from_slice(&(36 + WAV_DECLARED_DATA).to_le_bytes());
    header.extend_from_slice(b"WAVEfmt ");
    header.extend_from_slice(&16_u32.to_le_bytes());
    header.extend_from_slice(&1_u16.to_le_bytes());
    header.extend_from_slice(&1_u16.to_le_bytes());
    header.extend_from_slice(&44_100_u32.to_le_bytes());
    header.extend_from_slice(&88_200_u32.to_le_bytes());
    header.extend_from_slice(&2_u16.to_le_bytes());
    header.extend_from_slice(&16_u16.to_le_bytes());
    header.extend_from_slice(b"data");
    header.extend_from_slice(&WAV_DECLARED_DATA.to_le_bytes());
    header
}

/// A FIFO with a writer that stays open and silent until released.
struct StuckFifo {
    release: Option<Sender<()>>,
    closer: Option<std::thread::JoinHandle<()>>,
}

impl StuckFifo {
    /// Makes the FIFO at `path` and writes the header into it. `O_RDWR` lets the
    /// test open it without a reader being there, and keeps `open` from ever
    /// blocking on either side.
    fn make(path: &std::path::Path) -> Self {
        let made = std::process::Command::new("mkfifo")
            .arg(path)
            .status()
            .unwrap();
        assert!(made.success(), "mkfifo failed");
        let mut writer = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_CLOEXEC)
            .open(path)
            .unwrap();
        writer.write_all(&wav_header()).unwrap();
        let (release, released) = channel::<()>();
        let path = path.to_owned();
        let closer = std::thread::spawn(move || {
            // Released by the test, or by the clock: whichever comes first.
            let _ = released.recv_timeout(STUCK_FOR);
            // The name goes first: a start over that reopens the FIFO then
            // finds nothing, instead of blocking in `open` for good.
            let _ = std::fs::remove_file(&path);
            drop(writer);
        });
        Self {
            release: Some(release),
            closer: Some(closer),
        }
    }
}

impl Drop for StuckFifo {
    fn drop(&mut self) {
        if let Some(release) = self.release.take() {
            let _ = release.send(());
        }
        if let Some(closer) = self.closer.take() {
            let _ = closer.join();
        }
    }
}

/// The longest stretch between two heartbeats.
fn longest_gap(beats: &[Instant]) -> Duration {
    beats
        .windows(2)
        .map(|pair| pair[1].duration_since(pair[0]))
        .max()
        .unwrap_or(Duration::ZERO)
}

#[test]
fn a_restart_of_a_start_stuck_in_a_read_does_not_freeze_the_main_context() {
    let mut harness = Harness::new();
    harness.player.start_watchdog.deadline = TEST_DEADLINE;
    let dir = tempfile::tempdir().unwrap();
    let fifo = dir.path().join("stuck.wav");
    // Declared after the harness, so it is dropped first: the fd is closed
    // before the harness stops a pipeline that may still be reading from it.
    let stuck = StuckFifo::make(&fifo);

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

    // The restart, or the end of the stuck period without one: the old code
    // fails to restart once the FIFO is gone, so waiting for the counter alone
    // would wait out the whole patience.
    let began = Instant::now();
    harness.pump_until(PATIENCE, |_| {
        harness.player.start_watchdog.restarts() >= 1 || began.elapsed() > STUCK_FOR * 2
    });
    harness.pump_for(WATCH_AFTER_RESTART);
    heartbeat.remove();

    let gap = longest_gap(&beats.lock().unwrap_or_else(PoisonError::into_inner));
    assert!(
        gap < MAX_GAP,
        "the main context stood still for {gap:?} while the watchdog restarted a stuck start"
    );
    assert!(
        harness.player.start_watchdog.restarts() >= 1,
        "the watchdog never restarted the stuck start, so the test measured nothing"
    );

    // The retired pipeline is the reaper's, and it finishes once the read does:
    // nothing is left running behind the test.
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
