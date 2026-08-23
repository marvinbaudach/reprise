//! Writing a performance report to a file that does not exist yet.
//!
//! The frontend takes the measurements, because only it knows when a frame
//! reached the screen. Putting bytes on disk is not its business, so the write
//! lives here.

use std::fs::OpenOptions;
use std::io::BufWriter;
use std::path::Path;

use serde::Serialize;

/// Serialises `value` as JSON into a newly created `path`.
///
/// An existing file is an error rather than something to overwrite: a report
/// path that is already taken is almost always left over from an earlier run,
/// and replacing it would destroy that run's evidence at the exact moment
/// somebody is comparing two of them.
pub fn write_new_json<T: Serialize>(path: &Path, value: &T) -> Result<(), serde_json::Error> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map(BufWriter::new)
        .map_err(serde_json::Error::io)
        .and_then(|writer| serde_json::to_writer(writer, value))
}

/// The host's one-minute load average, as `/proc/loadavg` reports it.
///
/// A performance number is only readable next to the load it was taken under,
/// so the oracle that prints one needs this. Reading `/proc` is not a view
/// concern either — the same reason the report write above lives here rather
/// than in the frontend.
///
/// Returns `"missing"` when the file cannot be read, so a failed reading of the
/// host never costs the diagnostic line it belongs to.
pub fn host_load_one_minute() -> String {
    std::fs::read_to_string("/proc/loadavg")
        .ok()
        .and_then(|contents| contents.split_whitespace().next().map(str::to_owned))
        .unwrap_or_else(|| "missing".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_report_round_trips_through_the_written_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("report.json");

        write_new_json(&path, &serde_json::json!({ "phases": [7] })).unwrap();

        let written: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(written["phases"][0], 7);
    }

    #[test]
    fn an_existing_report_is_never_overwritten() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("report.json");
        write_new_json(&path, &serde_json::json!({ "run": "first" })).unwrap();

        let second = write_new_json(&path, &serde_json::json!({ "run": "second" }));

        assert!(second.is_err());
        let kept: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(kept["run"], "first");
    }

    #[test]
    fn an_unwritable_directory_is_reported_rather_than_panicking() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("missing").join("report.json");

        let result = write_new_json(&path, &serde_json::json!({}));

        assert!(result.is_err());
    }
}
