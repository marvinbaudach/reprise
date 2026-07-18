//! Isolated generated-metadata scalability baseline for Reprise queries.
//!
//! The caller must provide a database path that does not exist. This keeps the
//! tool fail-closed: it can never benchmark against (or seed) the user's real
//! library by accident. `scripts/performance-baseline.sh` supplies a path in a
//! private temporary directory and runs the documented 10,000/100,000-track
//! scenarios.

use std::error::Error;
use std::path::PathBuf;
use std::time::Instant;

use reprise_core::queries;
use reprise_core::view_source::ViewSource;
use rusqlite::Connection;
use serde::Serialize;

const USAGE: &str =
    "usage: scalability_baseline --db <path> --tracks <count> [--iterations <count>]";
const DEFAULT_ITERATIONS: usize = 5;
const MAX_ITERATIONS: usize = 100;
const MAX_GENERATED_TRACKS: usize = 1_000_000;
const WINDOW_ROWS: i64 = 200;

#[derive(Debug, PartialEq, Eq)]
struct Config {
    db_path: PathBuf,
    track_count: usize,
    iterations: usize,
}

impl Config {
    fn parse<I, S>(args: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut args = args.into_iter().map(Into::into);
        let mut db_path = None;
        let mut track_count = None;
        let mut iterations = DEFAULT_ITERATIONS;

        while let Some(argument) = args.next() {
            match argument.as_str() {
                "--db" => {
                    db_path = Some(PathBuf::from(args.next().ok_or_else(|| USAGE.to_string())?));
                }
                "--tracks" => {
                    track_count = Some(parse_bounded_count(
                        &args.next().ok_or_else(|| USAGE.to_string())?,
                        "--tracks",
                        MAX_GENERATED_TRACKS,
                    )?);
                }
                "--iterations" => {
                    iterations = parse_bounded_count(
                        &args.next().ok_or_else(|| USAGE.to_string())?,
                        "--iterations",
                        MAX_ITERATIONS,
                    )?;
                }
                _ => return Err(format!("unknown argument: {argument}")),
            }
        }

        Ok(Self {
            db_path: db_path.ok_or_else(|| USAGE.to_string())?,
            track_count: track_count.ok_or_else(|| USAGE.to_string())?,
            iterations,
        })
    }
}

fn parse_bounded_count(value: &str, name: &str, maximum: usize) -> Result<usize, String> {
    let parsed = value
        .parse::<usize>()
        .map_err(|_| format!("{name} must be between 1 and {maximum}"))?;
    if !(1..=maximum).contains(&parsed) {
        return Err(format!("{name} must be between 1 and {maximum}"));
    }
    Ok(parsed)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct TimingSummary {
    min_us: u64,
    median_us: u64,
    max_us: u64,
    result_rows: usize,
}

impl TimingSummary {
    fn from_samples(mut samples: Vec<u64>, result_rows: usize) -> Self {
        assert!(!samples.is_empty(), "timing samples must not be empty");
        samples.sort_unstable();
        Self {
            min_us: samples[0],
            median_us: samples[samples.len() / 2],
            max_us: samples[samples.len() - 1],
            result_rows,
        }
    }
}

#[derive(Debug, Serialize)]
struct BaselineReport {
    schema_version: u32,
    generated_tracks: usize,
    database_bytes: u64,
    iterations: usize,
    startup: TimingSummary,
    library_count: TimingSummary,
    first_window: TimingSummary,
    middle_window: TimingSummary,
    final_window: TimingSummary,
    filtered_count: TimingSummary,
    library_stats: TimingSummary,
    playback_ids: TimingSummary,
}

fn seed_generated_metadata(conn: &mut Connection, track_count: usize) -> rusqlite::Result<()> {
    let tx = conn.transaction()?;
    {
        let mut insert = tx.prepare_cached(
            "INSERT INTO tracks (path, title, artist, album, album_artist, genre, year, \
             track_no, duration_ms, rating, play_count, added_at) \
             VALUES (?1, ?2, ?3, ?4, ?3, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        )?;
        for index in 0..track_count {
            let title_prefix = if index.is_multiple_of(997) {
                "Needle"
            } else {
                "Track"
            };
            insert.execute(rusqlite::params![
                format!("/synthetic/library/track-{index:06}.flac"),
                format!("{title_prefix} {index:06}"),
                format!("Artist {:04}", index % 1_000),
                format!("Album {:05}", index % 10_000),
                format!("Genre {:02}", index % 20),
                1980 + i64::try_from(index % 47).unwrap_or(0),
                1 + i64::try_from(index % 20).unwrap_or(0),
                180_000 + i64::try_from(index % 120_000).unwrap_or(0),
                i64::try_from(index % 6).unwrap_or(0),
                i64::try_from(index % 500).unwrap_or(0),
                i64::try_from(index).unwrap_or(i64::MAX),
            ])?;
        }
    }
    tx.commit()
}

fn elapsed_us(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX)
}

fn measure<F>(iterations: usize, mut operation: F) -> Result<TimingSummary, Box<dyn Error>>
where
    F: FnMut() -> Result<usize, Box<dyn Error>>,
{
    let mut samples = Vec::with_capacity(iterations);
    let mut expected_rows = None;
    for _ in 0..iterations {
        let started = Instant::now();
        let result_rows = operation()?;
        samples.push(elapsed_us(started));
        match expected_rows {
            Some(expected) if expected != result_rows => {
                return Err(format!(
                    "measurement result changed between iterations: {expected} != {result_rows}"
                )
                .into());
            }
            None => expected_rows = Some(result_rows),
            Some(_) => {}
        }
    }
    Ok(TimingSummary::from_samples(
        samples,
        expected_rows.unwrap_or(0),
    ))
}

fn run(config: &Config) -> Result<BaselineReport, Box<dyn Error>> {
    if config.db_path.exists() {
        return Err(format!(
            "refusing to use an existing database: {}",
            config.db_path.display()
        )
        .into());
    }

    let mut conn = reprise_core::db::open_migrated(Some(&config.db_path))?;
    seed_generated_metadata(&mut conn, config.track_count)?;
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")?;
    drop(conn);

    let startup = measure(config.iterations, || {
        let conn = reprise_core::db::open_migrated(Some(&config.db_path))?;
        let count: i64 = conn.query_row("SELECT count(*) FROM tracks", [], |row| row.get(0))?;
        let count = usize::try_from(count).unwrap_or(usize::MAX);
        if count != config.track_count {
            return Err(format!(
                "startup opened {count} rows, expected {}",
                config.track_count
            )
            .into());
        }
        Ok(1)
    })?;

    let mut conn = reprise_core::db::open_migrated(Some(&config.db_path))?;
    let library_count = measure(config.iterations, || {
        let count = queries::query_track_count(&conn, &ViewSource::Library, "", &[])?;
        Ok(usize::try_from(count).unwrap_or(usize::MAX))
    })?;

    let first_window = measure_window(&mut conn, config.iterations, 0)?;
    let middle_offset = i64::try_from(config.track_count / 2).unwrap_or(i64::MAX);
    let middle_window = measure_window(&mut conn, config.iterations, middle_offset)?;
    let final_offset =
        i64::try_from(config.track_count.saturating_sub(WINDOW_ROWS as usize)).unwrap_or(i64::MAX);
    let final_window = measure_window(&mut conn, config.iterations, final_offset)?;

    let filtered_count = measure(config.iterations, || {
        let count = queries::query_track_count(&conn, &ViewSource::Library, "needle", &[])?;
        Ok(usize::try_from(count).unwrap_or(usize::MAX))
    })?;
    let library_stats = measure(config.iterations, || {
        let stats = queries::query_library_stats(&conn, "")?;
        Ok(usize::try_from(stats.track_count).unwrap_or(usize::MAX))
    })?;
    let playback_ids = measure(config.iterations, || {
        let ids = queries::query_track_ids(&conn, &ViewSource::Library, "title", "asc", "", &[])?;
        Ok(ids.len())
    })?;

    Ok(BaselineReport {
        schema_version: 1,
        generated_tracks: config.track_count,
        database_bytes: std::fs::metadata(&config.db_path)?.len(),
        iterations: config.iterations,
        startup,
        library_count,
        first_window,
        middle_window,
        final_window,
        filtered_count,
        library_stats,
        playback_ids,
    })
}

fn measure_window(
    conn: &mut Connection,
    iterations: usize,
    offset: i64,
) -> Result<TimingSummary, Box<dyn Error>> {
    measure(iterations, || {
        let rows = queries::query_track_window(
            conn,
            &ViewSource::Library,
            "title",
            "asc",
            "",
            offset,
            WINDOW_ROWS,
            &[],
        )?;
        Ok(rows.len())
    })
}

fn main() -> Result<(), Box<dyn Error>> {
    let config = Config::parse(std::env::args().skip(1))?;
    let report = run(&config)?;
    serde_json::to_writer(std::io::stdout().lock(), &report)?;
    println!();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_requires_an_explicit_database_and_track_count() {
        assert_eq!(
            Config::parse(Vec::<String>::new()).unwrap_err(),
            "usage: scalability_baseline --db <path> --tracks <count> [--iterations <count>]"
        );
    }

    #[test]
    fn cli_accepts_bounded_generated_metadata_runs() {
        let config = Config::parse([
            "--db",
            "/tmp/reprise-performance/test.db",
            "--tracks",
            "100000",
            "--iterations",
            "7",
        ])
        .unwrap();

        assert_eq!(
            config,
            Config {
                db_path: "/tmp/reprise-performance/test.db".into(),
                track_count: 100_000,
                iterations: 7,
            }
        );
    }

    #[test]
    fn cli_rejects_unbounded_track_and_iteration_counts() {
        assert_eq!(
            Config::parse(["--db", "/tmp/x.db", "--tracks", "1000001"]).unwrap_err(),
            "--tracks must be between 1 and 1000000"
        );
        assert_eq!(
            Config::parse([
                "--db",
                "/tmp/x.db",
                "--tracks",
                "10000",
                "--iterations",
                "0",
            ])
            .unwrap_err(),
            "--iterations must be between 1 and 100"
        );
    }

    #[test]
    fn timing_summary_reports_sorted_min_median_and_max() {
        assert_eq!(
            TimingSummary::from_samples(vec![90, 10, 50], 200),
            TimingSummary {
                min_us: 10,
                median_us: 50,
                max_us: 90,
                result_rows: 200,
            }
        );
    }

    #[test]
    fn report_serializes_the_stable_json_contract() {
        let report = BaselineReport {
            schema_version: 1,
            generated_tracks: 10_000,
            database_bytes: 42,
            iterations: 3,
            startup: TimingSummary::from_samples(vec![3, 1, 2], 1),
            library_count: TimingSummary::from_samples(vec![6, 4, 5], 10_000),
            first_window: TimingSummary::from_samples(vec![9, 7, 8], 200),
            middle_window: TimingSummary::from_samples(vec![12, 10, 11], 200),
            final_window: TimingSummary::from_samples(vec![15, 13, 14], 200),
            filtered_count: TimingSummary::from_samples(vec![18, 16, 17], 11),
            library_stats: TimingSummary::from_samples(vec![21, 19, 20], 10_000),
            playback_ids: TimingSummary::from_samples(vec![24, 22, 23], 10_000),
        };

        assert_eq!(
            serde_json::to_value(report).unwrap(),
            serde_json::json!({
                "schema_version": 1,
                "generated_tracks": 10000,
                "database_bytes": 42,
                "iterations": 3,
                "startup": {"min_us": 1, "median_us": 2, "max_us": 3, "result_rows": 1},
                "library_count": {"min_us": 4, "median_us": 5, "max_us": 6, "result_rows": 10000},
                "first_window": {"min_us": 7, "median_us": 8, "max_us": 9, "result_rows": 200},
                "middle_window": {"min_us": 10, "median_us": 11, "max_us": 12, "result_rows": 200},
                "final_window": {"min_us": 13, "median_us": 14, "max_us": 15, "result_rows": 200},
                "filtered_count": {"min_us": 16, "median_us": 17, "max_us": 18, "result_rows": 11},
                "library_stats": {"min_us": 19, "median_us": 20, "max_us": 21, "result_rows": 10000},
                "playback_ids": {"min_us": 22, "median_us": 23, "max_us": 24, "result_rows": 10000}
            })
        );
    }
}
