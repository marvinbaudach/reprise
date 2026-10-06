//! The decodes running right now, keyed by track id. A decode is registered
//! for its whole lifetime, foreground and backfill alike, so the partial
//! picture of the playing track can be read whichever of them is producing it
//! (`progress.rs`), and so a track change can stop the outgoing foreground
//! decode (`supersede_foreground_track_analysis`).
//!
//! Nothing here is persisted: a registry entry holds the live sink and dies
//! with the decode.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use reprise_core::spectrogram::SPECTROGRAM_FRAME_RATE_HZ;

use crate::MusicLibrary;

use super::compute::AnalysisPcmSink;

const MILLIS_PER_SECOND: i64 = 1_000;

/// The track's length in spectrogram frames, `ceil(duration * 20 Hz)`, or
/// `None` when the duration is unknown (zero or negative): without it the
/// peak buckets cannot be placed, so such a track reports no progress.
pub(crate) fn expected_frame_count(duration_ms: i64) -> Option<usize> {
    if duration_ms <= 0 {
        return None;
    }
    let frame_milliseconds = duration_ms.saturating_mul(i64::from(SPECTROGRAM_FRAME_RATE_HZ));
    usize::try_from((frame_milliseconds + MILLIS_PER_SECOND - 1) / MILLIS_PER_SECOND).ok()
}

struct RegisteredDecode {
    sink: Arc<AnalysisPcmSink>,
    expected_frames: Option<usize>,
    background: bool,
}

/// A running decode's sink and expected length, copied out of the registry so
/// the caller can read the sink without holding the registry lock.
pub(crate) struct DecodeHandle {
    pub(crate) sink: Arc<AnalysisPcmSink>,
    pub(crate) expected_frames: usize,
}

#[derive(Default)]
pub(crate) struct DecodeRegistry {
    entries: Mutex<HashMap<i64, RegisteredDecode>>,
}

/// Removes its track's entry when the decode returns, whatever the outcome.
pub(crate) struct DecodeRegistration<'a> {
    registry: &'a DecodeRegistry,
    track_id: i64,
}

impl DecodeRegistry {
    fn entries(&self) -> std::sync::MutexGuard<'_, HashMap<i64, RegisteredDecode>> {
        self.entries.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Publishes `sink` as the decode of `track_id`. `AnalysisInFlight` lets
    /// one caller at a time decode a track, so the entry is never contended.
    pub(crate) fn register(
        &self,
        track_id: i64,
        sink: &Arc<AnalysisPcmSink>,
        expected_frames: Option<usize>,
        background: bool,
    ) -> DecodeRegistration<'_> {
        self.entries().insert(
            track_id,
            RegisteredDecode {
                sink: Arc::clone(sink),
                expected_frames,
                background,
            },
        );
        DecodeRegistration {
            registry: self,
            track_id,
        }
    }

    /// The running decode of `track_id`, if there is one with an expected
    /// length to place a partial picture against.
    pub(crate) fn lookup(&self, track_id: i64) -> Option<DecodeHandle> {
        let entries = self.entries();
        let decode = entries.get(&track_id)?;
        Some(DecodeHandle {
            sink: Arc::clone(&decode.sink),
            expected_frames: decode.expected_frames?,
        })
    }

    /// Stops every foreground decode whose track is not `keep`, marking each
    /// as superseded. Never touches the backfill (it yields on its own), never
    /// waits for a decode to end: it only flips flags.
    pub(crate) fn supersede_foreground_except(&self, keep: Option<i64>) {
        for (track_id, decode) in self.entries().iter() {
            if !decode.background && Some(*track_id) != keep {
                tracing::debug!(track_id, "superseding the foreground track analysis");
                decode.sink.supersede();
            }
        }
    }
}

impl Drop for DecodeRegistration<'_> {
    fn drop(&mut self) {
        self.registry.entries().remove(&self.track_id);
    }
}

#[uniffi::export]
impl MusicLibrary {
    /// Stops the foreground analysis of every track except `keep_track_id`:
    /// the playing track changed, so the outgoing track's decode is no longer
    /// wanted. Its waiters get `Superseded` and nothing is stored, so the
    /// track stays pending for the backfill. The library-wide backfill is
    /// never touched, and the call returns at once.
    pub fn supersede_foreground_track_analysis(&self, keep_track_id: Option<i64>) {
        self.analysis_in_flight
            .decodes()
            .supersede_foreground_except(keep_track_id);
    }
}
