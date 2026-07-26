//! Sequential MP3 transcoding for Android device synchronization.

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use gstreamer as gst;
use gstreamer::prelude::*;
use reprise_core::device_sync::{Mp3Quality, SyncTrack};

const PIPELINE_DESCRIPTION: &str = "uridecodebin name=decoder ! audioconvert ! audioresample ! \
    lamemp3enc name=encoder target=bitrate cbr=true ! id3v2mux name=mux ! \
    filesink name=output";
const BUS_POLL_INTERVAL: gst::ClockTime = gst::ClockTime::from_mseconds(100);

/// Factories that must be present before a sync may perform destructive work.
pub const REQUIRED_MP3_FACTORIES: [&str; 6] = [
    "uridecodebin",
    "audioconvert",
    "audioresample",
    "lamemp3enc",
    "id3v2mux",
    "filesink",
];

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Mp3Metadata {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub track_number: Option<u32>,
    pub cover: Option<Vec<u8>>,
}

impl Mp3Metadata {
    pub fn for_track(track: &SyncTrack) -> Self {
        Self {
            title: track.title.clone(),
            artist: track.artist.clone(),
            album: track.album.clone(),
            album_artist: track.album_artist.clone(),
            track_number: track.track_number,
            cover: reprise_core::cover::read_cover_tag(&track.source_path).picture,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mp3TranscodeRequest {
    pub source: PathBuf,
    pub output: PathBuf,
    pub quality: Mp3Quality,
    pub metadata: Mp3Metadata,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscodedFile {
    pub path: PathBuf,
    pub size_bytes: u64,
}

#[derive(Debug)]
pub enum TranscodeError {
    MissingFactories(Vec<String>),
    InvalidPath,
    OutputExists(PathBuf),
    Gstreamer(String),
    Io(std::io::Error),
    Cancelled,
}

impl fmt::Display for TranscodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingFactories(factories) => {
                write!(
                    formatter,
                    "required GStreamer factories are missing: {}",
                    factories.join(", ")
                )
            }
            Self::InvalidPath => formatter.write_str("transcode path is not representable"),
            Self::OutputExists(path) => {
                write!(
                    formatter,
                    "transcode output already exists: {}",
                    path.display()
                )
            }
            Self::Gstreamer(error) => write!(formatter, "GStreamer transcode failed: {error}"),
            Self::Io(error) => write!(formatter, "transcode file I/O failed: {error}"),
            Self::Cancelled => formatter.write_str("transcode cancelled"),
        }
    }
}

impl std::error::Error for TranscodeError {}

impl From<std::io::Error> for TranscodeError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// Proves that the fixed MP3/ID3 pipeline can be constructed on this host.
pub fn probe_mp3_transcode_capability() -> Result<(), TranscodeError> {
    gst::init().map_err(|error| TranscodeError::Gstreamer(error.to_string()))?;
    let missing = REQUIRED_MP3_FACTORIES
        .iter()
        .filter(|factory| gst::ElementFactory::find(factory).is_none())
        .map(|factory| (*factory).to_string())
        .collect::<Vec<_>>();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(TranscodeError::MissingFactories(missing))
    }
}

/// Decodes one local source into one CBR MP3 with an ID3v2 header.
///
/// This function is blocking and must run on a dedicated worker. Its one-file
/// API is deliberate: the caller completes transcode, MTP copy, verification,
/// finalization and inventory before starting the next track.
pub fn transcode_to_mp3(
    request: &Mp3TranscodeRequest,
    cancelled: &AtomicBool,
) -> Result<TranscodedFile, TranscodeError> {
    if cancelled.load(Ordering::SeqCst) {
        return Err(TranscodeError::Cancelled);
    }
    if request.output.exists() {
        return Err(TranscodeError::OutputExists(request.output.clone()));
    }
    probe_mp3_transcode_capability()?;
    let source_uri = gst::glib::filename_to_uri(&request.source, None)
        .map_err(|_| TranscodeError::InvalidPath)?
        .to_string();
    let output_path = request.output.to_str().ok_or(TranscodeError::InvalidPath)?;
    let pipeline = gst::parse::launch(PIPELINE_DESCRIPTION)
        .map_err(|error| TranscodeError::Gstreamer(error.to_string()))?
        .downcast::<gst::Pipeline>()
        .map_err(|_| TranscodeError::Gstreamer("parser did not create a pipeline".into()))?;
    let cleanup = PipelineCleanup(pipeline);
    cleanup
        .0
        .by_name("decoder")
        .ok_or_else(|| TranscodeError::Gstreamer("missing decoder".into()))?
        .set_property("uri", source_uri);
    cleanup
        .0
        .by_name("encoder")
        .ok_or_else(|| TranscodeError::Gstreamer("missing MP3 encoder".into()))?
        .set_property(
            "bitrate",
            i32::try_from(request.quality.kbps()).unwrap_or(i32::MAX),
        );
    cleanup
        .0
        .by_name("output")
        .ok_or_else(|| TranscodeError::Gstreamer("missing file output".into()))?
        .set_property("location", output_path);
    apply_metadata(&cleanup.0, &request.metadata)?;

    finish_output(&request.output, run_pipeline(&cleanup.0, cancelled))
}

fn finish_output(
    output: &Path,
    pipeline_result: Result<(), TranscodeError>,
) -> Result<TranscodedFile, TranscodeError> {
    if let Err(error) = pipeline_result {
        let _ = std::fs::remove_file(output);
        return Err(error);
    }
    let size_bytes = match std::fs::metadata(output) {
        Ok(metadata) => metadata.len(),
        Err(error) => {
            let _ = std::fs::remove_file(output);
            return Err(error.into());
        }
    };
    Ok(TranscodedFile {
        path: output.to_path_buf(),
        size_bytes,
    })
}

struct PipelineCleanup(gst::Pipeline);

impl Drop for PipelineCleanup {
    fn drop(&mut self) {
        let _ = self.0.set_state(gst::State::Null);
    }
}

fn apply_metadata(pipeline: &gst::Pipeline, metadata: &Mp3Metadata) -> Result<(), TranscodeError> {
    let mux = pipeline
        .by_name("mux")
        .ok_or_else(|| TranscodeError::Gstreamer("missing ID3v2 muxer".into()))?
        .dynamic_cast::<gst::TagSetter>()
        .map_err(|_| TranscodeError::Gstreamer("ID3v2 muxer cannot accept tags".into()))?;
    mux.set_tag_merge_mode(gst::TagMergeMode::Replace);
    mux.add_tag::<gst::tags::Title>(&metadata.title.as_str(), gst::TagMergeMode::Replace);
    mux.add_tag::<gst::tags::Artist>(&metadata.artist.as_str(), gst::TagMergeMode::Replace);
    mux.add_tag::<gst::tags::Album>(&metadata.album.as_str(), gst::TagMergeMode::Replace);
    mux.add_tag::<gst::tags::AlbumArtist>(
        &metadata.album_artist.as_str(),
        gst::TagMergeMode::Replace,
    );
    if let Some(track_number) = metadata.track_number {
        mux.add_tag::<gst::tags::TrackNumber>(&track_number, gst::TagMergeMode::Replace);
    }
    if let Some(cover) = &metadata.cover {
        let caps = gst::Caps::builder(cover_mime_type(cover)).build();
        let buffer = gst::Buffer::from_slice(cover.clone());
        let sample = gst::Sample::builder().buffer(&buffer).caps(&caps).build();
        mux.add_tag::<gst::tags::Image>(&sample, gst::TagMergeMode::Replace);
    }
    Ok(())
}

fn cover_mime_type(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        "image/png"
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        "image/jpeg"
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        "image/gif"
    } else {
        "application/octet-stream"
    }
}

fn run_pipeline(pipeline: &gst::Pipeline, cancelled: &AtomicBool) -> Result<(), TranscodeError> {
    pipeline
        .set_state(gst::State::Playing)
        .map_err(|error| TranscodeError::Gstreamer(error.to_string()))?;
    let bus = pipeline
        .bus()
        .ok_or_else(|| TranscodeError::Gstreamer("pipeline has no bus".into()))?;
    loop {
        if cancelled.load(Ordering::SeqCst) {
            return Err(TranscodeError::Cancelled);
        }
        let Some(message) = bus.timed_pop_filtered(
            BUS_POLL_INTERVAL,
            &[gst::MessageType::Eos, gst::MessageType::Error],
        ) else {
            continue;
        };
        match message.view() {
            gst::MessageView::Eos(_) => return Ok(()),
            gst::MessageView::Error(error) => {
                return Err(TranscodeError::Gstreamer(format!(
                    "{} ({:?})",
                    error.error(),
                    error.debug()
                )));
            }
            _ => {}
        }
    }
}

#[cfg(test)]
#[path = "device_transfer_mp3_tests.rs"]
mod mp3_tests;
