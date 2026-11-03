//! Soak evidence for the start watchdog (#1239): a production `Player` makes
//! many sequential local starts, whole-file and CUE, while burner threads
//! saturate the CPU. Nothing is injected, so the hangs are the real ones.
//!
//! Both tests are `#[ignore]`d and the gates do not run them:
//!
//! ```text
//! cargo test -p reprise-platform-linux start_watchdog_soak -- --ignored --nocapture --test-threads=1
//! ```
//!
//! `REPRISE_START_SOAK_STARTS` overrides the number of starts per arm. The
//! production arm fails when a start stays silent past the watchdog's give-up
//! bound; the control arm puts the deadline out of reach and only reports,
//! because the hangs it finds are its result.

use std::sync::atomic::AtomicBool;

use super::segment_support::{cue_item, encode_flac, write_regions_wav, Harness};
use super::*;
use crate::player::start_watchdog::{allowed_idle, START_ATTEMPTS, START_DEADLINE};

const DEFAULT_STARTS: usize = 300;
const STARTS_ENV_VAR: &str = "REPRISE_START_SOAK_STARTS";
/// Long enough that no start reaches its end while it is being judged.
const TRACK_MS: u32 = 30_000;
const CUE_SEGMENT: (i64, i64) = (2_000, 20_000);
/// Slack on top of the longest the watchdog may take to give a start up.
const GIVE_UP_SLACK: Duration = Duration::from_secs(5);
/// A deadline no run reaches: the watchdog is armed but never acts.
const OUT_OF_REACH: Duration = Duration::from_secs(24 * 60 * 60);
/// Burner threads per core, so the player has to share every core.
const BURNERS_PER_CORE: usize = 2;

fn starts() -> usize {
    std::env::var(STARTS_ENV_VAR)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(DEFAULT_STARTS)
}

/// Spins `BURNERS_PER_CORE` threads per core until dropped.
struct Burners {
    stop: Arc<AtomicBool>,
    threads: Vec<std::thread::JoinHandle<()>>,
}

impl Burners {
    fn start() -> Self {
        let cores = std::thread::available_parallelism().map_or(1, usize::from);
        let stop = Arc::new(AtomicBool::new(false));
        let threads = (0..cores * BURNERS_PER_CORE)
            .map(|_| {
                let stop = stop.clone();
                std::thread::spawn(move || {
                    let mut spin = 0_u64;
                    while !stop.load(Ordering::Relaxed) {
                        spin = std::hint::black_box(spin.wrapping_mul(31).wrapping_add(7));
                    }
                })
            })
            .collect();
        Self { stop, threads }
    }
}

impl Drop for Burners {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
    }
}

/// How one arm went.
#[derive(Default)]
struct Summary {
    starts: usize,
    /// Starts whose pipeline reached PLAYING.
    played: usize,
    give_ups: usize,
    /// Starts that ended in neither, within the bound.
    silent: usize,
    slowest: Duration,
}

fn playing_pipeline(player: &Player) -> bool {
    let playbin = player
        .playbin
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    playbin.current_state() == gst::State::Playing
}

fn gave_up(events: &[PlayerEvent]) -> bool {
    events.iter().any(|event| {
        matches!(
            event,
            PlayerEvent::Error(failure)
                if failure.kind() == PlaybackFailureKind::StartNeverFinished
        )
    })
}

/// Makes `starts()` sequential starts on a production player whose watchdog
/// deadline is `deadline`, and returns the summary and the restarts performed.
fn soak(deadline: Duration) -> (Summary, u64) {
    let mut harness = Harness::new();
    harness.player.start_watchdog.deadline = deadline;
    let dir = tempfile::tempdir().unwrap();
    let wav = dir.path().join("album.wav");
    let flac = dir.path().join("album.flac");
    write_regions_wav(&wav, &[(TRACK_MS, true)]);
    encode_flac(&wav, &flac);
    let flac_path = flac.to_str().unwrap().to_owned();
    // The longest a watchdog that restarts has to take to give a start up.
    let give_up_bound: Duration = (1..=START_ATTEMPTS)
        .map(|attempt| allowed_idle(deadline.min(START_DEADLINE), attempt))
        .sum();
    let bound = give_up_bound + GIVE_UP_SLACK;
    let _burners = Burners::start();

    let mut summary = Summary::default();
    for index in 0..starts() {
        let began = std::time::Instant::now();
        let cue = index % 2 == 1;
        let played = if cue {
            harness.player.play(cue_item(&flac, CUE_SEGMENT, 0.0))
        } else {
            harness.player.play(item(&flac_path))
        };
        played.unwrap();
        let events = harness.pump_until(bound, |events| {
            playing_pipeline(&harness.player) || gave_up(events)
        });
        let took = began.elapsed();
        summary.starts += 1;
        summary.slowest = summary.slowest.max(took);
        if gave_up(&events) {
            summary.give_ups += 1;
        } else if playing_pipeline(&harness.player) {
            summary.played += 1;
        } else {
            summary.silent += 1;
            eprintln!("start {index} (cue: {cue}) stayed silent for {took:?}");
        }
    }
    let restarts = harness.player.start_watchdog.restarts();
    (summary, restarts)
}

fn report(arm: &str, summary: &Summary, restarts: u64) {
    println!(
        "start watchdog soak [{arm}]: starts={} played={} restarts={restarts} give_ups={} silent={} slowest={:.2?}",
        summary.starts, summary.played, summary.give_ups, summary.silent, summary.slowest
    );
}

#[test]
#[ignore = "soak: minutes of CPU saturation; run by hand"]
fn start_watchdog_soak_production_arm() {
    let (summary, restarts) = soak(START_DEADLINE);
    report("production", &summary, restarts);
    assert_eq!(
        summary.silent, 0,
        "every start must reach PLAYING or be given up on within the give-up bound"
    );
}

#[test]
#[ignore = "soak: minutes of CPU saturation; run by hand"]
fn start_watchdog_soak_control_arm() {
    let (summary, restarts) = soak(OUT_OF_REACH);
    report("control (watchdog out of reach)", &summary, restarts);
}
