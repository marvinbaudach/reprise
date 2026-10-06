//! One decode, many tracks: the render data of the tracks cut from a single audio
//! file by a CUE sheet.
//!
//! A [`SegmentedRenderDataSession`] takes the PCM of the whole file in stream
//! order, as [`RenderDataSession`] does, and hands each stretch of it to the
//! session of the track it belongs to. Every track therefore gets exactly the
//! peaks, spectrogram and loudness that analysing that stretch on its own would
//! give, from a single decode of the file.

use crate::render_data_session::{RenderDataSession, RenderDataSessionError};
use crate::waveform::TrackRenderData;

/// The part of a file one track covers, in milliseconds from its start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SegmentBounds {
    pub start_ms: i64,
    pub end_ms: i64,
}

struct Track {
    bounds: SegmentBounds,
    session: RenderDataSession,
}

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
    /// was cut into chunks.
    pub fn push_pcm_i16(
        &mut self,
        samples: &[i16],
        sample_rate_hz: u32,
        channel_count: u32,
    ) -> Result<(), RenderDataSessionError> {
        self.route(samples, sample_rate_hz, channel_count, |session, part| {
            session.push_pcm_i16(part, sample_rate_hz, channel_count)
        })
    }

    /// Feeds interleaved floating-point PCM; see [`push_pcm_i16`](Self::push_pcm_i16).
    pub fn push_pcm_f32(
        &mut self,
        samples: &[f32],
        sample_rate_hz: u32,
        channel_count: u32,
    ) -> Result<(), RenderDataSessionError> {
        self.route(samples, sample_rate_hz, channel_count, |session, part| {
            session.push_pcm_f32(part, sample_rate_hz, channel_count)
        })
    }

    fn route<T>(
        &mut self,
        samples: &[T],
        sample_rate_hz: u32,
        channel_count: u32,
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
        let chunk_start = self.frames_seen;
        let chunk_end = chunk_start + frames;
        for track in &mut self.tracks {
            let from = frame_at(track.bounds.start_ms, sample_rate_hz).max(chunk_start);
            let to = frame_at(track.bounds.end_ms, sample_rate_hz).min(chunk_end);
            if from < to {
                let first = ((from - chunk_start) as usize) * channels;
                let last = ((to - chunk_start) as usize) * channels;
                push(&mut track.session, &samples[first..last])?;
            }
        }
        self.frames_seen = chunk_end;
        Ok(())
    }

    /// Ends the stream and returns the render data of each segment, in the order
    /// the segments were given. A segment the stream never reached reads as an
    /// empty stream; the others are unaffected.
    #[must_use]
    pub fn finish(self) -> Vec<Result<TrackRenderData, RenderDataSessionError>> {
        self.tracks
            .into_iter()
            .map(|track| track.session.finish())
            .collect()
    }
}

/// The first frame at or after `milliseconds` of a stream at `sample_rate_hz`.
fn frame_at(milliseconds: i64, sample_rate_hz: u32) -> u64 {
    let milliseconds = u128::try_from(milliseconds.max(0)).unwrap_or(0);
    u64::try_from(milliseconds * u128::from(sample_rate_hz) / 1_000).unwrap_or(u64::MAX)
}

#[cfg(test)]
#[path = "render_data_segments_tests.rs"]
mod tests;
