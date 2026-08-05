//! Measures what the averaging window does to a real track's colour curve.
//!
//! The diagnosis this answers: taken per bar, the spectral centroid swings from
//! beat to beat, so neighbouring bars land far apart on the axis — cyan beside
//! magenta inside about two seconds of music. That is noise, and noise forms no
//! pattern anyone can read. This runs the shipped smoothing over a stored curve
//! at the track's real duration and reports, for both the raw and the averaged
//! curve, how far neighbouring display bars jump and how long a run of one
//! colour lasts.
//!
//! Reads the database read-only, so it is safe to point at the live one.
//!
//! Usage: cargo run --example spectral_seek_smoothing_proof -- <db> [tracks]

use reprise_core::db::Db;
use reprise_core::waveform_cache::centroid_for_playback;
use reprise_view::spectral_colour::{
    section_boundaries, shape_centroid, smooth_centroid_over_seconds, spectral_colour,
    CENTROID_WINDOW_S,
};

/// The bar count a full-width player bar settles on.
const DISPLAY_BARS: usize = 160;
/// Two bars count as the same field while they stay this close on the axis.
const SAME_FIELD: f32 = 0.06;

fn hex(colour: (f64, f64, f64)) -> String {
    let channel = |value: f64| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!(
        "#{:02X}{:02X}{:02X}",
        channel(colour.0),
        channel(colour.1),
        channel(colour.2)
    )
}

/// The largest jump between neighbouring display bars, as a fraction of the
/// whole axis.
fn worst_neighbour_jump(bars: &[f32]) -> f32 {
    bars.windows(2)
        .map(|pair| (pair[0] - pair[1]).abs())
        .fold(0.0, f32::max)
}

/// The longest run of bars that stay within `SAME_FIELD` of where the run
/// started, in seconds.
fn longest_field_seconds(bars: &[f32], duration_s: f64) -> f64 {
    if bars.is_empty() {
        return 0.0;
    }
    let per_bar = duration_s / bars.len() as f64;
    let mut best = 1usize;
    let mut run = 1usize;
    let mut anchor = bars[0];
    for bar in &bars[1..] {
        if (bar - anchor).abs() <= SAME_FIELD {
            run += 1;
        } else {
            best = best.max(run);
            run = 1;
            anchor = *bar;
        }
    }
    best.max(run) as f64 * per_bar
}

fn main() {
    let mut args = std::env::args().skip(1);
    let path = std::path::PathBuf::from(args.next().expect("database path"));
    let limit: usize = args.next().map_or(8, |value| value.parse().expect("count"));

    let db = Db::open_ready_read_only(&path).expect("open database read-only");
    // A second, read-only handle just to pick the tracks: `Db` keeps its own
    // connection private, and this example has no business reaching into it.
    let listing = rusqlite::Connection::open_with_flags(
        &path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )
    .expect("open listing connection");
    let mut statement = listing
        .prepare(
            "SELECT t.id, t.title, t.duration_ms FROM tracks t \
             JOIN track_spectrograms s ON s.track_id = t.id \
             WHERE t.duration_ms > 60000 ORDER BY t.id LIMIT ?1",
        )
        .expect("prepare");
    let rows: Vec<(i64, String, i64)> = statement
        .query_map([limit as i64], |row| {
            Ok((
                row.get(0)?,
                row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                row.get(2)?,
            ))
        })
        .expect("query")
        .map(|row| row.expect("row"))
        .collect();

    println!(
        "window = {CENTROID_WINDOW_S} s, {DISPLAY_BARS} display bars\n\
         {:<34} {:>6} {:>14} {:>14} {:>12} {:>12}",
        "track", "len", "raw jump", "avg jump", "raw field", "avg field"
    );
    for (id, title, duration_ms) in rows {
        let Some(curve) = centroid_for_playback(&db, id, 1000) else {
            continue;
        };
        let duration_s = duration_ms as f64 / 1_000.0;
        let smoothed = smooth_centroid_over_seconds(&curve, duration_s, CENTROID_WINDOW_S);
        let raw_bars = shape_centroid(&curve, DISPLAY_BARS);
        let smooth_bars = shape_centroid(&smoothed, DISPLAY_BARS);
        let title: String = title.chars().take(32).collect();
        println!(
            "{title:<34} {:>5.0}s {:>13.1}% {:>13.1}% {:>11.1}s {:>11.1}s",
            duration_s,
            worst_neighbour_jump(&raw_bars) * 100.0,
            worst_neighbour_jump(&smooth_bars) * 100.0,
            longest_field_seconds(&raw_bars, duration_s),
            longest_field_seconds(&smooth_bars, duration_s),
        );
        let marks = section_boundaries(&smoothed, duration_s);
        println!(
            "    {} section marks — one every {:.0} s",
            marks.len(),
            if marks.is_empty() {
                duration_s
            } else {
                duration_s / marks.len() as f64
            }
        );
        let swatch = |bars: &[f32], at: f64| {
            hex(spectral_colour(f64::from(
                bars[((bars.len() as f64 * at) as usize).min(bars.len() - 1)],
            )))
        };
        println!(
            "    raw {} {} {} {}   averaged {} {} {} {}",
            swatch(&raw_bars, 0.10),
            swatch(&raw_bars, 0.11),
            swatch(&raw_bars, 0.12),
            swatch(&raw_bars, 0.13),
            swatch(&smooth_bars, 0.10),
            swatch(&smooth_bars, 0.11),
            swatch(&smooth_bars, 0.12),
            swatch(&smooth_bars, 0.13),
        );
    }
}
