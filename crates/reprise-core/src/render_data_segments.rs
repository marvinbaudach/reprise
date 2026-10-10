//! One decode, many tracks: the render data of the tracks cut from a single audio
//! file by a CUE sheet.
//!
//! A [`SegmentedRenderDataSession`] takes the PCM of the whole file in stream
//! order, as [`RenderDataSession`] does, and hands each stretch of it to the
//! session of the track it belongs to. Every track therefore gets exactly the
//! peaks, spectrogram and loudness that analysing that stretch on its own would
//! give, from a single decode of the file.
//!
//! A chunk is placed by its timestamp where the decoder gives one, so a chunk
//! the decoder dropped does not shift every later track: the stretch it fell in
//! simply has fewer samples, and no silence is invented for it. Audio that
//! arrives for a stretch already passed is dropped — the first copy wins, since
//! what came earlier has already been measured. A chunk without a timestamp
//! continues from the running frame count.

use crate::render_data_session::{PartialSource, RenderDataSession, RenderDataSessionError};
use crate::waveform::TrackRenderData;

/// The part of a file one track covers, in milliseconds from its start.
///
/// `last_in_file` marks the file's last track. It plays and is analysed to the
/// decoded end of the file whatever `end_ms` says, because that end is only
/// the duration the file's metadata claims: a claim a few seconds short would
/// drop the tail of the album, and one too long would wait for audio that
/// never comes. `start_ms` and `end_ms` remain the cut the catalog records.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SegmentBounds {
    pub start_ms: i64,
    pub end_ms: i64,
    pub last_in_file: bool,
}

impl SegmentBounds {
    /// Whether `other` is the same cut of the file, whatever either says about
    /// being the last track.
    #[must_use]
    pub fn same_cut(&self, other: &Self) -> bool {
        (self.start_ms, self.end_ms) == (other.start_ms, other.end_ms)
    }
}

struct Track {
    bounds: SegmentBounds,
    session: RenderDataSession,
}

/// How far a chunk's timestamp may sit from where the running frame count puts
/// it and still count as following on directly. Decoders round their
/// timestamps; a chunk they drop is a whole buffer, tens of milliseconds.
const CONTIGUOUS_TOLERANCE_US: i64 = 1_000;

const MICROSECONDS_PER_SECOND: i128 = 1_000_000;

pub struct SegmentedRenderDataSession {
    tracks: Vec<Track>,
    /// Frames (one sample per channel) of the stream consumed so far.
    frames_seen: u64,
    config: Option<(u32, u32)>,
}

impl SegmentedRenderDataSession {
    /// A full analysis (peaks, spectrogram, loudness) of each of `segments`.
    #[must_use]
    pub fn new(segments: &[SegmentBounds], peak_count: usize) -> Self {
        Self::with_sessions(segments, || RenderDataSession::with_peak_count(peak_count))
    }

    /// One session per segment, each made by `make`, for callers that want
    /// something other than the full analysis.
    #[must_use]
    pub fn with_sessions(segments: &[SegmentBounds], make: impl Fn() -> RenderDataSession) -> Self {
        Self {
            tracks: segments
                .iter()
                .map(|bounds| Track {
                    bounds: *bounds,
                    session: make(),
                })
                .collect(),
            frames_seen: 0,
            config: None,
        }
    }

    /// Feeds interleaved 16-bit PCM. Chunks may be any size and may straddle the
    /// boundary between two tracks; the result does not depend on how the stream
    /// was cut into chunks. `start_us` is the chunk's presentation time in
    /// microseconds from the start of the file, `None` when the decoder gave
    /// none; see the module documentation for how it places the chunk.
    pub fn push_pcm_i16(
        &mut self,
        samples: &[i16],
        sample_rate_hz: u32,
        channel_count: u32,
        start_us: Option<i64>,
    ) -> Result<(), RenderDataSessionError> {
        self.route(
            samples,
            sample_rate_hz,
            channel_count,
            start_us,
            |session, part| session.push_pcm_i16(part, sample_rate_hz, channel_count),
        )
    }

    /// Feeds interleaved floating-point PCM; see [`push_pcm_i16`](Self::push_pcm_i16).
    pub fn push_pcm_f32(
        &mut self,
        samples: &[f32],
        sample_rate_hz: u32,
        channel_count: u32,
        start_us: Option<i64>,
    ) -> Result<(), RenderDataSessionError> {
        self.route(
            samples,
            sample_rate_hz,
            channel_count,
            start_us,
            |session, part| session.push_pcm_f32(part, sample_rate_hz, channel_count),
        )
    }

    fn route<T>(
        &mut self,
        samples: &[T],
        sample_rate_hz: u32,
        channel_count: u32,
        start_us: Option<i64>,
        mut push: impl FnMut(&mut RenderDataSession, &[T]) -> Result<(), RenderDataSessionError>,
    ) -> Result<(), RenderDataSessionError> {
        if sample_rate_hz == 0 || channel_count == 0 {
            return Err(RenderDataSessionError::InvalidStreamConfig);
        }
        match self.config {
            Some(config) if config != (sample_rate_hz, channel_count) => {
                return Err(RenderDataSessionError::RateOrChannelChanged);
            }
            _ => self.config = Some((sample_rate_hz, channel_count)),
        }
        let channels = channel_count as usize;
        let frames = (samples.len() / channels) as u64;
        let placed = self.placement(start_us, sample_rate_hz);
        // Whatever lies before the frames already measured arrived late.
        let late = u64::try_from(i128::from(self.frames_seen) - placed)
            .unwrap_or(0)
            .min(frames);
        let samples = &samples[(late as usize) * channels..];
        let chunk_start = u64::try_from(placed).unwrap_or(0).max(self.frames_seen);
        let chunk_end = chunk_start + (frames - late);
        for track in &mut self.tracks {
            let from = frame_at(track.bounds.start_ms, sample_rate_hz).max(chunk_start);
            let to = if track.bounds.last_in_file {
                chunk_end
            } else {
                frame_at(track.bounds.end_ms, sample_rate_hz).min(chunk_end)
            };
            if from < to {
                let first = ((from - chunk_start) as usize) * channels;
                let last = ((to - chunk_start) as usize) * channels;
                push(&mut track.session, &samples[first..last])?;
            }
        }
        self.frames_seen = self.frames_seen.max(chunk_end);
        Ok(())
    }

    /// The frame a chunk starting at `start_us` begins at. A timestamp within
    /// [`CONTIGUOUS_TOLERANCE_US`] of the running count is taken as the count.
    fn placement(&self, start_us: Option<i64>, sample_rate_hz: u32) -> i128 {
        let seen = i128::from(self.frames_seen);
        let Some(start_us) = start_us else {
            return seen;
        };
        let placed = frame_at_us(start_us, sample_rate_hz);
        let tolerance = frame_at_us(CONTIGUOUS_TOLERANCE_US, sample_rate_hz);
        if (placed - seen).abs() <= tolerance {
            seen
        } else {
            placed
        }
    }

    /// What has been decoded so far of the segment at `index`, in the order the
    /// segments were given: the partial picture of one track while its file
    /// is still being decoded. `None` for an index past the last segment.
    #[must_use]
    pub fn partial_source(&self, index: usize) -> Option<PartialSource> {
        self.tracks
            .get(index)
            .map(|track| track.session.partial_source())
    }

    /// Ends the stream and returns the render data of each segment, in the order
    /// the segments were given. A segment the stream never reached reads as an
    /// empty stream; the others are unaffected. The file's last track is
    /// complete with whatever the stream held up to its end, and says where
    /// that end was ([`TrackRenderData::decoded_end_ms`]).
    #[must_use]
    pub fn finish(self) -> Vec<Result<TrackRenderData, RenderDataSessionError>> {
        let decoded_end_ms = self
            .config
            .map(|(sample_rate_hz, _)| end_ms_of(self.frames_seen, sample_rate_hz));
        self.tracks
            .into_iter()
            .map(|track| {
                let last_in_file = track.bounds.last_in_file;
                track.session.finish().map(|data| TrackRenderData {
                    decoded_end_ms: decoded_end_ms.filter(|_| last_in_file),
                    ..data
                })
            })
            .collect()
    }
}

/// The millisecond nearest to the end of the first `frames` frames of a stream
/// at `sample_rate_hz`.
fn end_ms_of(frames: u64, sample_rate_hz: u32) -> i64 {
    let rate = u128::from(sample_rate_hz.max(1));
    let milliseconds = (u128::from(frames) * 1_000 + rate / 2) / rate;
    i64::try_from(milliseconds).unwrap_or(i64::MAX)
}

/// The first frame at or after `milliseconds` of a stream at `sample_rate_hz`.
fn frame_at(milliseconds: i64, sample_rate_hz: u32) -> u64 {
    let milliseconds = u128::try_from(milliseconds.max(0)).unwrap_or(0);
    u64::try_from(milliseconds * u128::from(sample_rate_hz) / 1_000).unwrap_or(u64::MAX)
}

/// The frame nearest to `microseconds` of a stream at `sample_rate_hz`;
/// negative before the stream's start.
fn frame_at_us(microseconds: i64, sample_rate_hz: u32) -> i128 {
    let scaled = i128::from(microseconds) * i128::from(sample_rate_hz);
    (scaled + MICROSECONDS_PER_SECOND / 2).div_euclid(MICROSECONDS_PER_SECOND)
}

#[cfg(test)]
#[path = "render_data_segments_tests.rs"]
mod tests;
