//! Proof that the menu item's worker produces a coloured seek bar.
//!
//! Runs the same backfill the "Analyze Library" item starts, against a copy of
//! a real database, and then asks the same question the seek bar asks: what
//! colour does this track's curve give at a few positions? Without a stored
//! spectrogram every position answers with the same fallback, which is exactly
//! the flat bar this work set out to fix.
//!
//! Usage: cargo run --example spectral_seek_proof -- <db-copy> <track-id>...

use reprise_core::db::Db;
use reprise_core::waveform_cache::centroid_for_playback;
use reprise_platform_linux::spectrogram_backfill::SpectrogramBackfillHandle;
use reprise_view::spectral_colour::{centroid_at, spectral_colour};

fn hex(colour: (f64, f64, f64)) -> String {
    let channel = |value: f64| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!(
        "#{:02X}{:02X}{:02X}",
        channel(colour.0),
        channel(colour.1),
        channel(colour.2)
    )
}

fn main() {
    let mut args = std::env::args().skip(1);
    let path = std::path::PathBuf::from(args.next().expect("database path"));
    let ids: Vec<i64> = args.map(|id| id.parse().expect("track id")).collect();

    let db = Db::open_ready(&path).expect("open database");
    for &id in &ids {
        match centroid_for_playback(&db, id, 1000) {
            Some(_) => println!("track {id}: already has a curve before the run"),
            None => println!("track {id}: no curve — the bar draws in the plain accent"),
        }
    }
    drop(db);

    let summary = SpectrogramBackfillHandle::start(path.clone())
        .join()
        .expect("backfill worker");
    println!(
        "\nrun finished: status={:?} stored={} failed={} source_changed={}\n",
        summary.status, summary.stored, summary.failed, summary.source_changed
    );

    let db = Db::open_ready(&path).expect("reopen database");
    for id in ids {
        let Some(curve) = centroid_for_playback(&db, id, 1000) else {
            println!("track {id}: STILL no curve");
            continue;
        };
        let swatches: Vec<String> = [0.0, 0.25, 0.5, 0.75, 1.0]
            .into_iter()
            .map(|fraction| hex(spectral_colour(centroid_at(&curve, fraction))))
            .collect();
        let distinct: std::collections::BTreeSet<&String> = swatches.iter().collect();
        println!(
            "track {id}: {} — {} distinct colour(s) across the bar",
            swatches.join(" "),
            distinct.len()
        );
    }
}
