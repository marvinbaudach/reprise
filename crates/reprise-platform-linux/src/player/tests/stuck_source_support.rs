//! Fixtures for the tests that put a source on a pipeline that blocks in the
//! kernel, the way a dead NFS/SMB mount does (#1323, #1327).
//!
//! A source blocks in one of two places, and a FIFO makes either:
//!
//! - in `open()`, when it has no writer: [`make_fifo`] with nobody on the other
//!   end, released by [`OpenerRelease`];
//! - in `read()`, when it has a writer that goes quiet: [`StuckFifo`], which
//!   writes a WAV header (and, for [`StuckFifo::after_playing`], about a second
//!   of silence first, so the pipeline reaches `Playing` before the source
//!   blocks).
//!
//! Every fixture lets go of its FIFO after a bounded time even if the test is
//! the thread that is stuck, so a regression stalls for that long and fails on
//! the gap instead of hanging the suite.

use std::io::Write;
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use gstreamer::prelude::*;

use super::segment_support::gain_of;

use gstreamer as gst;
use std::sync::PoisonError;

use crate::player::reaper;

/// How often the heartbeat timer asks the main context to dispatch it.
pub(super) const HEARTBEAT: Duration = Duration::from_millis(20);
/// The longest the main context may go without dispatching the heartbeat.
pub(super) const MAX_GAP: Duration = Duration::from_millis(500);
/// How long a FIFO's writer stays open, whatever the test does: a regression
/// freezes the main context for about this long, and no longer.
pub(super) const STUCK_FOR: Duration = Duration::from_secs(3);
/// How long the heartbeat is watched once the restart has happened.
pub(super) const WATCH_AFTER_RESTART: Duration = Duration::from_millis(700);
/// Far more than a test needs, and far less than a hang would take.
pub(super) const PATIENCE: Duration = Duration::from_secs(20);

/// The sample rate and size the WAV header declares; no sample ever follows
/// beyond what a fixture writes.
const WAV_HEADER_LEN: u32 = 44;
const WAV_DECLARED_DATA: u32 = 1 << 24;
const SAMPLE_RATE: u32 = 44_100;
const BYTES_PER_SAMPLE: u32 = 2;
/// About a second of 16-bit mono silence.
const SILENCE_BYTES: usize = (SAMPLE_RATE * BYTES_PER_SAMPLE) as usize;
/// Room for the header and the silence in the pipe, so the test's own writes
/// never block (the system's maximum for an unprivileged process is 1 MiB).
const PIPE_CAPACITY_BYTES: libc::c_int = 1 << 20;

/// A 16-bit mono WAV header that promises far more data than is ever written.
pub(super) fn wav_header() -> Vec<u8> {
    let mut header = Vec::with_capacity(WAV_HEADER_LEN as usize);
    header.extend_from_slice(b"RIFF");
    header.extend_from_slice(&(36 + WAV_DECLARED_DATA).to_le_bytes());
    header.extend_from_slice(b"WAVEfmt ");
    header.extend_from_slice(&16_u32.to_le_bytes());
    header.extend_from_slice(&1_u16.to_le_bytes());
    header.extend_from_slice(&1_u16.to_le_bytes());
    header.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    header.extend_from_slice(&(SAMPLE_RATE * BYTES_PER_SAMPLE).to_le_bytes());
    header.extend_from_slice(&2_u16.to_le_bytes());
    header.extend_from_slice(&16_u16.to_le_bytes());
    header.extend_from_slice(b"data");
    header.extend_from_slice(&WAV_DECLARED_DATA.to_le_bytes());
    header
}

/// Makes the pipe behind `writer` big enough for `len` bytes, so writing them
/// never blocks the test, whatever has or has not been read.
fn grow_pipe_to_hold(writer: &std::fs::File, len: usize) {
    // SAFETY: `fcntl` with `F_SETPIPE_SZ` only reads the integer argument and
    // the descriptor, which `writer` keeps open for the call.
    let capacity =
        unsafe { libc::fcntl(writer.as_raw_fd(), libc::F_SETPIPE_SZ, PIPE_CAPACITY_BYTES) };
    assert!(
        usize::try_from(capacity).is_ok_and(|bytes| bytes >= len),
        "the pipe cannot hold the fixture's content, so writing it would block"
    );
}

/// A FIFO with a writer that stays open and silent until released.
pub(super) struct StuckFifo {
    release: Option<Sender<()>>,
    closer: Option<std::thread::JoinHandle<()>>,
}

impl StuckFifo {
    /// Makes the FIFO at `path` and writes the header into it. `O_RDWR` lets the
    /// test open it without a reader being there, and keeps `open` from ever
    /// blocking on either side.
    pub(super) fn make(path: &std::path::Path) -> Self {
        Self::open_for(path, STUCK_FOR)
    }

    /// [`Self::make`], with the writer closed after `open_for` if nobody
    /// releases it sooner.
    pub(super) fn open_for(path: &std::path::Path, open_for: Duration) -> Self {
        Self::with_content(path, open_for, &wav_header())
    }

    /// [`Self::make`], with about a second of silent samples behind the header:
    /// a pipeline reads them, prerolls and reaches `Playing`, and only then
    /// does its source block in `read()`.
    pub(super) fn after_playing(path: &std::path::Path) -> Self {
        Self::with_content(path, STUCK_FOR, &header_and_a_second_of_silence())
    }

    fn with_content(path: &std::path::Path, open_for: Duration, content: &[u8]) -> Self {
        make_fifo(path);
        let mut writer = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_CLOEXEC)
            .open(path)
            .unwrap();
        grow_pipe_to_hold(&writer, content.len());
        writer.write_all(content).unwrap();
        let (release, released) = channel::<()>();
        let path = path.to_owned();
        let closer = std::thread::spawn(move || {
            // Released by the test, or by the clock: whichever comes first.
            let _ = released.recv_timeout(open_for);
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
pub(super) fn longest_gap(beats: &[Instant]) -> Duration {
    beats
        .windows(2)
        .map(|pair| pair[1].duration_since(pair[0]))
        .max()
        .unwrap_or(Duration::ZERO)
}

/// A `glib::timeout_add` heartbeat on the main context, recording when it
/// fired. Only fires while something pumps the main context.
pub(super) struct Heartbeat {
    beats: Arc<Mutex<Vec<Instant>>>,
    source: Option<gst::glib::SourceId>,
}

impl Heartbeat {
    pub(super) fn start() -> Self {
        let beats = Arc::new(Mutex::new(vec![Instant::now()]));
        let recorded = beats.clone();
        let source = gst::glib::timeout_add(HEARTBEAT, move || {
            recorded
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(Instant::now());
            gst::glib::ControlFlow::Continue
        });
        Self {
            beats,
            source: Some(source),
        }
    }

    /// The longest gap so far, the stretch up to now included.
    pub(super) fn longest_gap(&self) -> Duration {
        let mut beats = self
            .beats
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        beats.push(Instant::now());
        longest_gap(&beats)
    }
}

impl Drop for Heartbeat {
    fn drop(&mut self) {
        if let Some(source) = self.source.take() {
            source.remove();
        }
    }
}

/// How many descriptors of this process point at `path`.
pub(super) fn open_descriptors_of(path: &std::path::Path) -> usize {
    std::fs::read_dir("/proc/self/fd").map_or(0, |fds| {
        fds.filter_map(Result::ok)
            .filter_map(|fd| std::fs::read_link(fd.path()).ok())
            .filter(|target| target == path)
            .count()
    })
}

pub(super) fn make_fifo(path: &std::path::Path) {
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
pub(super) fn release_openers(path: &std::path::Path) {
    let writer = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_CLOEXEC)
        .open(path);
    let _ = std::fs::remove_file(path);
    drop(writer);
}

/// Releases the blocked openers at `path` when told to or after `STUCK_FOR`.
pub(super) struct OpenerRelease {
    release: Option<Sender<()>>,
    releaser: Option<std::thread::JoinHandle<()>>,
}

impl OpenerRelease {
    pub(super) fn at(path: &std::path::Path) -> Self {
        Self::after(path, STUCK_FOR)
    }

    /// [`Self::at`], with the clock set to `release_after`.
    pub(super) fn after(path: &std::path::Path, release_after: Duration) -> Self {
        let (release, released) = channel::<()>();
        let path = path.to_owned();
        let releaser = std::thread::spawn(move || {
            let _ = released.recv_timeout(release_after);
            release_openers(&path);
        });
        Self {
            release: Some(release),
            releaser: Some(releaser),
        }
    }

    /// Releases the openers now rather than when dropped.
    pub(super) fn release_now(&mut self) {
        if let Some(release) = self.release.take() {
            let _ = release.send(());
        }
        if let Some(releaser) = self.releaser.take() {
            let _ = releaser.join();
        }
    }
}

impl Drop for OpenerRelease {
    fn drop(&mut self) {
        self.release_now();
    }
}

/// Waits for every retired pipeline to reach `Null`, which they do once the
/// sources are released, and fails if one never does.
pub(super) fn assert_every_retired_pipeline_went_down() {
    let waited = Instant::now();
    while reaper::in_flight::count() > 0 && waited.elapsed() < PATIENCE {
        std::thread::sleep(HEARTBEAT);
    }
    assert_eq!(
        reaper::in_flight::count(),
        0,
        "a retired pipeline was never taken down"
    );
}

/// The writer that finally shows up for a FIFO whose reader is blocked in
/// `open()`: it opens the FIFO (which wakes the reader), writes `content` and
/// stays open and silent, so the reader then blocks in `read()` for the rest.
/// Dropped, it takes the name away and closes.
pub(super) struct LateWriter {
    path: std::path::PathBuf,
    writer: Option<std::fs::File>,
}

impl LateWriter {
    pub(super) fn arrive(path: &std::path::Path, content: &[u8]) -> Self {
        let writer = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_CLOEXEC)
            .open(path)
            .ok()
            .map(|mut writer| {
                grow_pipe_to_hold(&writer, content.len());
                let _ = writer.write_all(content);
                writer
            });
        Self {
            path: path.to_owned(),
            writer,
        }
    }
}

impl Drop for LateWriter {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
        drop(self.writer.take());
    }
}

/// About a second of 16-bit mono silence behind a WAV header: what a source
/// that played for a moment before it stalled has delivered.
pub(super) fn header_and_a_second_of_silence() -> Vec<u8> {
    let mut content = wav_header();
    content.resize(content.len() + SILENCE_BYTES, 0);
    content
}

/// A stall inside the streaming thread, on a source that can be sought: a
/// FIFO cannot be (its seek is refused at once), so a seek test stalls the
/// streaming thread in a buffer probe instead. Like a `filesrc` blocked in
/// `read()`, the thread keeps the stream lock a flushing seek needs, and a
/// flush does not wake it. The stall lets go when told to, or by the clock.
#[derive(Clone, Default)]
pub(super) struct StreamStall {
    state: Arc<(Mutex<StallState>, Condvar)>,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum StallState {
    #[default]
    Idle,
    Armed,
    Released,
}

impl StreamStall {
    /// Puts the stalling probe on the gain element's sink pad of `playbin`.
    pub(super) fn install_on(&self, playbin: &gst::Element) {
        let state = self.state.clone();
        gain_of(playbin).static_pad("sink").unwrap().add_probe(
            gst::PadProbeType::BUFFER,
            move |_, _| {
                let (lock, released) = &*state;
                let mut guard = lock.lock().unwrap_or_else(PoisonError::into_inner);
                if *guard == StallState::Armed {
                    let (woken, _) = released
                        .wait_timeout_while(guard, STUCK_FOR, |state| *state == StallState::Armed)
                        .unwrap_or_else(PoisonError::into_inner);
                    guard = woken;
                }
                drop(guard);
                gst::PadProbeReturn::Ok
            },
        );
    }

    /// The next buffer to reach the probe stays there until [`Self::release`].
    pub(super) fn arm(&self) {
        *self.state.0.lock().unwrap_or_else(PoisonError::into_inner) = StallState::Armed;
    }

    pub(super) fn release(&self) {
        *self.state.0.lock().unwrap_or_else(PoisonError::into_inner) = StallState::Released;
        self.state.1.notify_all();
    }
}

impl Drop for StreamStall {
    fn drop(&mut self) {
        if Arc::strong_count(&self.state) == 1 {
            self.release();
        }
    }
}
