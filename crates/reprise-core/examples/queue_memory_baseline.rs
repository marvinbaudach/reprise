//! Fresh-process memory probe for Reprise's in-memory playback queue.

use std::error::Error;

use reprise_core::queue::Queue;
use serde::Serialize;

const USAGE: &str = "usage: queue_memory_baseline --tracks <count>";
const MAX_GENERATED_TRACKS: usize = 1_000_000;

#[derive(Debug, PartialEq, Eq)]
struct Config {
    track_count: usize,
}

impl Config {
    fn parse<I, S>(args: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut args = args.into_iter().map(Into::into);
        let mut track_count = None;
        while let Some(argument) = args.next() {
            match argument.as_str() {
                "--tracks" => {
                    let value = args.next().ok_or_else(|| USAGE.to_string())?;
                    let parsed = value.parse::<usize>().map_err(|_| {
                        format!("--tracks must be between 1 and {MAX_GENERATED_TRACKS}")
                    })?;
                    if !(1..=MAX_GENERATED_TRACKS).contains(&parsed) {
                        return Err(format!(
                            "--tracks must be between 1 and {MAX_GENERATED_TRACKS}"
                        ));
                    }
                    track_count = Some(parsed);
                }
                _ => return Err(format!("unknown argument: {argument}")),
            }
        }
        Ok(Self {
            track_count: track_count.ok_or_else(|| USAGE.to_string())?,
        })
    }
}

#[derive(Debug, Serialize)]
struct QueueMemoryReport {
    schema_version: u32,
    generated_tracks: usize,
    logical_payload_bytes: usize,
    rss_before_bytes: u64,
    rss_after_bytes: u64,
    rss_delta_bytes: u64,
    rss_delta_bytes_per_track: f64,
}

fn logical_payload_bytes(track_count: usize) -> usize {
    track_count.saturating_mul(std::mem::size_of::<i64>() + std::mem::size_of::<usize>())
}

fn resident_bytes() -> Result<u64, Box<dyn Error>> {
    let status = std::fs::read_to_string("/proc/self/status")?;
    let line = status
        .lines()
        .find(|line| line.starts_with("VmRSS:"))
        .ok_or("/proc/self/status has no VmRSS field")?;
    let kibibytes = line
        .split_whitespace()
        .nth(1)
        .ok_or("VmRSS has no numeric value")?
        .parse::<u64>()?;
    Ok(kibibytes.saturating_mul(1024))
}

fn run(config: &Config) -> Result<QueueMemoryReport, Box<dyn Error>> {
    let rss_before_bytes = resident_bytes()?;
    let ids: Vec<i64> = (1..=config.track_count)
        .map(|id| i64::try_from(id).unwrap_or(i64::MAX))
        .collect();
    let mut queue = Queue::new();
    queue.set_tracks(ids, 0);
    std::hint::black_box(queue.current());
    std::hint::black_box(queue.len());
    let rss_after_bytes = resident_bytes()?;
    let rss_delta_bytes = rss_after_bytes.saturating_sub(rss_before_bytes);

    Ok(QueueMemoryReport {
        schema_version: 1,
        generated_tracks: config.track_count,
        logical_payload_bytes: logical_payload_bytes(config.track_count),
        rss_before_bytes,
        rss_after_bytes,
        rss_delta_bytes,
        rss_delta_bytes_per_track: rss_delta_bytes as f64 / config.track_count as f64,
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
    fn logical_queue_payload_accounts_for_ids_and_play_order() {
        assert_eq!(logical_payload_bytes(100_000), 1_600_000);
    }

    #[test]
    fn cli_accepts_only_bounded_generated_queue_sizes() {
        assert_eq!(
            Config::parse(["--tracks", "100000"]).unwrap(),
            Config {
                track_count: 100_000
            }
        );
        assert_eq!(
            Config::parse(["--tracks", "0"]).unwrap_err(),
            "--tracks must be between 1 and 1000000"
        );
    }

    #[test]
    fn report_serializes_the_stable_queue_memory_contract() {
        let report = QueueMemoryReport {
            schema_version: 1,
            generated_tracks: 100_000,
            logical_payload_bytes: 1_600_000,
            rss_before_bytes: 10_000_000,
            rss_after_bytes: 12_000_000,
            rss_delta_bytes: 2_000_000,
            rss_delta_bytes_per_track: 20.0,
        };

        assert_eq!(
            serde_json::to_value(report).unwrap(),
            serde_json::json!({
                "schema_version": 1,
                "generated_tracks": 100000,
                "logical_payload_bytes": 1600000,
                "rss_before_bytes": 10000000,
                "rss_after_bytes": 12000000,
                "rss_delta_bytes": 2000000,
                "rss_delta_bytes_per_track": 20.0
            })
        );
    }
}
