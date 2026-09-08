//! Writes one `.reprise-analysis` sidecar for an audio file, without a sync.
//!
//! The desktop normally produces sidecars inside device sync, which reads the
//! spectrogram and the waveform peaks back out of the library database. A file
//! that is not in the library therefore has no sidecar and the phone falls back
//! to a plain seek bar. This example decodes the file directly with the same
//! GStreamer backend the backfill uses and encodes the same Core format, so a
//! single track can be given its analysis without touching the library.
//!
//!     cargo run -p reprise-platform-linux --example analysis_sidecar -- \
//!         "<audio file>" ["<sidecar output>"]
//!
//! With no output path the sidecar is written beside the input with its
//! extension replaced, which is exactly the pairing the phone's scanner looks
//! for.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use reprise_core::device_sync::analysis_sidecar::{self, AnalysisSidecar};
use reprise_core::spectrogram::{TrackSourceFingerprint, SPECTROGRAM_BAND_COUNT};
use reprise_core::waveform::{RenderDataBackend, STORED_PEAK_COUNT};
use reprise_platform_linux::waveform::GstreamerWaveformBackend;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let Some(input) = arguments.next().map(PathBuf::from) else {
        return Err("usage: analysis_sidecar <audio file> [sidecar output]".into());
    };
    let output = match arguments.next() {
        Some(path) => PathBuf::from(path),
        None => default_output(&input)?,
    };

    let source = fingerprint(&input)?;
    let data = GstreamerWaveformBackend.extract_render_data_cancellable(
        &input,
        STORED_PEAK_COUNT,
        &AtomicBool::new(false),
    )?;

    let frames = data.spectrogram.cells().len() / SPECTROGRAM_BAND_COUNT;
    let sidecar = AnalysisSidecar::new(source, data.spectrogram, data.waveform_peaks);
    let bytes = sidecar.encode()?;
    // A sidecar that cannot be read back is a silent plain-seek-bar on the
    // phone, so the round trip is checked here rather than on the device.
    if AnalysisSidecar::decode(&bytes)? != sidecar {
        return Err("the encoded sidecar did not decode back to itself".into());
    }
    std::fs::write(&output, &bytes)?;

    println!("wrote {}", output.display());
    println!(
        "  {} bytes, {frames} spectrogram frames of {SPECTROGRAM_BAND_COUNT} bands, \
         {} waveform peaks",
        bytes.len(),
        sidecar.waveform_peaks.len(),
    );
    println!(
        "  source fingerprint: mtime {}, size {}",
        sidecar.source.mtime_seconds, sidecar.source.size_bytes,
    );
    Ok(())
}

fn default_output(input: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let name = analysis_sidecar::device_path_for_track(&input.to_string_lossy())
        .ok_or("the input path has no file name")?;
    Ok(input.with_file_name(
        Path::new(&name)
            .file_name()
            .ok_or("the sidecar name has no file name")?,
    ))
}

/// Mirrors the library's own file identity so the sidecar carries the source it
/// was actually computed from.
fn fingerprint(path: &Path) -> Result<TrackSourceFingerprint, Box<dyn std::error::Error>> {
    use std::os::unix::fs::MetadataExt;

    let metadata = std::fs::metadata(path)?;
    let mtime_seconds = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |duration| duration.as_secs() as i64);
    Ok(TrackSourceFingerprint {
        mtime_seconds,
        size_bytes: metadata.len() as i64,
        device: Some(metadata.dev() as i64),
        inode: Some(metadata.ino() as i64),
    })
}
