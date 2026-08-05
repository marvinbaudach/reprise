//! Headless probe: does the sound similarity actually put like next to like?
//!
//! Not a test — a measuring instrument. It fills a *copy* of a real library
//! with rendering data, then writes the ranked neighbours of a sample of seed
//! tracks to stdout as CSV so an outside script can score them against genre,
//! artist and album without this file knowing anything about scoring.
//!
//! Usage:
//!   cargo run --release --example sound_similarity_check -- <db-path> [flags]
//!
//!   --backfill              derive spectrograms and features first (slow)
//!   --seeds <n>             how many evenly spaced seed tracks (default 400)
//!   --limit <n>             neighbours per seed (default 7)
//!   --exclusions none|product   product = the shipped default (same album off)
//!   --weights default|timbre|dynamics

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use reprise_core::db::Db;
use reprise_core::sound_distance::DistanceWeights;
use reprise_core::sound_neighbours::{
    load_sound_candidates, rank_sound_neighbours, SoundNeighbourOptions,
};
use reprise_core::sound_stats::compute_sound_stats;
use reprise_core::spectrogram_backfill::run_render_data_backfill;
use reprise_platform_linux::waveform::GstreamerWaveformBackend;

fn flag_value(name: &str) -> Option<String> {
    let mut args = std::env::args();
    while let Some(argument) = args.next() {
        if argument == name {
            return args.next();
        }
    }
    None
}

fn has_flag(name: &str) -> bool {
    std::env::args().any(|argument| argument == name)
}

fn main() {
    let database_path = PathBuf::from(
        std::env::args()
            .nth(1)
            .expect("usage: sound_similarity_check <db-path> [flags]"),
    );
    let seed_count: usize = flag_value("--seeds")
        .and_then(|value| value.parse().ok())
        .unwrap_or(400);
    let limit: usize = flag_value("--limit")
        .and_then(|value| value.parse().ok())
        .unwrap_or(7);
    let exclusions = flag_value("--exclusions").unwrap_or_else(|| "product".into());
    let weights_argument = flag_value("--weights").unwrap_or_else(|| "default".into());
    let weights = match weights_argument.as_str() {
        "timbre" => DistanceWeights::TIMBRE,
        "dynamics" => DistanceWeights::DYNAMICS,
        // An explicit split, so the shipped numbers can be probed for
        // sensitivity without a rebuild per candidate.
        custom if custom.contains(',') => {
            let parts: Vec<f32> = custom
                .split(',')
                .map(|part| part.trim().parse().expect("weights are four numbers"))
                .collect();
            assert_eq!(parts.len(), 4, "expected band,timbre,dynamics,rhythm");
            DistanceWeights {
                band: parts[0],
                timbre: parts[1],
                dynamics: parts[2],
                rhythm: parts[3],
                tempo: 0.0,
            }
        }
        _ => DistanceWeights::DEFAULT,
    };

    if has_flag("--backfill") {
        let db = Db::open_migrated(Some(&database_path)).expect("open library for backfill");
        let cancelled = AtomicBool::new(false);
        let started = std::time::Instant::now();
        let summary =
            run_render_data_backfill(&db, &GstreamerWaveformBackend, &cancelled, |progress| {
                if progress.completed % 25 == 0 || progress.completed == progress.total {
                    let elapsed = started.elapsed().as_secs_f64();
                    let rate = if elapsed > 0.0 {
                        progress.completed as f64 / elapsed
                    } else {
                        0.0
                    };
                    let remaining = progress.total.saturating_sub(progress.completed) as f64;
                    eprintln!(
                        "backfill {}/{} ({:.2} tracks/s, ~{:.0} min left)",
                        progress.completed,
                        progress.total,
                        rate,
                        if rate > 0.0 {
                            remaining / rate / 60.0
                        } else {
                            0.0
                        }
                    );
                }
            })
            .expect("backfill runs to completion");
        eprintln!(
            "backfill summary: {summary:?} in {:.1} min",
            started.elapsed().as_secs_f64() / 60.0
        );
    }

    let db = Db::open_migrated(Some(&database_path)).expect("open library for ranking");
    let candidates = load_sound_candidates(&db).expect("load sound candidates");
    eprintln!("candidates with a sound profile: {}", candidates.len());
    if candidates.is_empty() {
        eprintln!("nothing to rank — run with --backfill first");
        return;
    }

    let features: Vec<_> = candidates
        .iter()
        .map(|candidate| candidate.features.clone())
        .collect();
    let stats = compute_sound_stats(&features);

    let options = SoundNeighbourOptions {
        exclude_same_album: exclusions == "product",
        exclude_same_artist: false,
        limit,
    };

    // Evenly spaced seeds: deterministic, no RNG, and it walks the whole
    // library instead of clustering in whatever the id order happens to be.
    let step = (candidates.len() / seed_count.max(1)).max(1);
    println!("seed_id,rank,neighbour_id,distance,percentile");
    let mut seeds = 0usize;
    for candidate in candidates.iter().step_by(step) {
        let result = rank_sound_neighbours(candidate, &candidates, &stats, weights, options);
        for (index, neighbour) in result.matches.iter().enumerate() {
            println!(
                "{},{},{},{:.6},{:.4}",
                candidate.track_id,
                index + 1,
                neighbour.track_id,
                neighbour.distance,
                neighbour.percentile
            );
        }
        seeds += 1;
    }
    eprintln!("ranked {seeds} seeds (limit {limit}, exclusions {exclusions})");
}
