//! The sink Kotlin's decoder pushes PCM into, the `TrackPcmDecoder` callback
//! it implements, and the compute-on-missing path `mobile_sync.rs` calls
//! when a sidecar import ends in `Missing`/`Invalid` (decision 5 of the
//! mother plan).

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, PoisonError};

use reprise_core::db::Db;
use reprise_core::render_data_session::RenderDataSession;

use crate::track_analysis::TrackAnalysisBackfill;
use crate::{LibraryError, MusicLibrary};

/// A decode failure crossing the FFI boundary. `Send + Sync` on the trait
/// below is what lets Kotlin's implementation live on whatever thread the
/// decode runs on; this error type is what crosses back.
#[derive(Clone, Debug, thiserror::Error, uniffi::Error)]
pub enum AnalysisDecodeError {
    #[error("decode failed: {detail}")]
    DecodeFailed { detail: String },
}

impl From<uniffi::UnexpectedUniFFICallbackError> for AnalysisDecodeError {
    fn from(error: uniffi::UnexpectedUniFFICallbackError) -> Self {
        Self::DecodeFailed {
            detail: error.to_string(),
        }
    }
}

/// Kotlin's platform decoder. Mirrors the live visualizer's PCM shape
/// (`visualizer.rs`'s `ingest_pcm_i16`): 16-bit interleaved PCM at the
/// stream's own rate and channel count, pushed until end of stream, until
/// [`AnalysisPcmSink::push_pcm_i16`] refuses a chunk, or until decoding
/// fails.
#[uniffi::export(callback_interface)]
pub trait TrackPcmDecoder: Send + Sync {
    /// Decodes `track_uri` from the start and pushes PCM into `sink`.
    /// `background` asks the decoder to lower the calling thread's priority
    /// for the call's duration (the library-wide backfill, never a
    /// foreground request).
    fn decode(
        &self,
        track_uri: String,
        sink: Arc<AnalysisPcmSink>,
        background: bool,
    ) -> Result<(), AnalysisDecodeError>;
}

/// Owns one track's [`RenderDataSession`] for the duration of one decode
/// call. `cancelled` is flipped from outside the decode call — by a
/// foreground request preempting the backfill's current item — so the
/// decoder can be told to stop without a second channel back into Kotlin.
#[derive(uniffi::Object)]
pub struct AnalysisPcmSink {
    session: Mutex<Option<RenderDataSession>>,
    cancelled: AtomicBool,
    /// Set when the session itself refused a chunk (a rate or channel
    /// change mid-stream): a data problem, distinct from the decoder giving
    /// up and distinct from being told to stop.
    refused: Mutex<Option<String>>,
}

impl AnalysisPcmSink {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            session: Mutex::new(Some(RenderDataSession::new())),
            cancelled: AtomicBool::new(false),
            refused: Mutex::new(None),
        })
    }

    pub(crate) fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    fn refused_reason(&self) -> Option<String> {
        self.refused
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Takes the session out and finishes it. Only ever called once, after
    /// the decode call has returned and cancellation has been ruled out.
    fn finish(&self) -> Result<reprise_core::waveform::TrackRenderData, String> {
        let session = self
            .session
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        session
            .expect("finish is called at most once per sink")
            .finish()
            .map_err(|error| error.to_string())
    }
}

#[uniffi::export]
impl AnalysisPcmSink {
    /// `false` tells the decoder to stop: either cancelled, or the session
    /// refused this chunk (a rate or channel change mid-stream).
    #[allow(clippy::needless_pass_by_value)] // UniFFI cannot export borrowed byte slices.
    pub fn push_pcm_i16(&self, bytes: Vec<u8>, sample_rate_hz: u32, channel_count: u32) -> bool {
        if self.is_cancelled() {
            return false;
        }
        let samples: Vec<i16> = bytes
            .chunks_exact(2)
            .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        let mut guard = self.session.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(session) = guard.as_mut() else {
            return false;
        };
        match session.push_pcm_i16(&samples, sample_rate_hz, channel_count) {
            Ok(()) => true,
            Err(error) => {
                drop(guard);
                *self.refused.lock().unwrap_or_else(PoisonError::into_inner) =
                    Some(error.to_string());
                false
            }
        }
    }
}

/// The Kotlin-facing outcome of one `import_track_analysis` call: the
/// sidecar outcomes it already reported, plus the compute path's own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum AndroidAnalysisOutcome {
    Imported,
    AlreadyImported,
    PhoneSourceChanged,
    Computed,
    DecodeFailed,
    NoDecoder,
    Cancelled,
}

/// One pending or finished decode, shared between every caller waiting on
/// the same track id.
type AnalysisCell = Arc<(Mutex<Option<AndroidAnalysisOutcome>>, Condvar)>;

/// The track id and sink of whichever item is currently decoding, published
/// as a single atomic write right before the decode call starts (decision in
/// finding review: splitting these into two independently-updated fields
/// left a window where a preemption check saw a track id with no sink yet
/// to cancel). `TrackAnalysisBackfill` owns the slot; `None` is idle.
pub(crate) type CurrentDecodeSlot = Mutex<Option<(i64, Arc<AnalysisPcmSink>)>>;

/// Deduplicates concurrent decodes of the same track: a second caller for a
/// track already being decoded waits for that decode's result rather than
/// starting a second one.
pub struct AnalysisInFlight {
    entries: Mutex<HashMap<i64, AnalysisCell>>,
}

pub(crate) enum Claim {
    /// Another caller already finished (or finishes while this one waits).
    Done(AndroidAnalysisOutcome),
    /// This caller now owns the decode; it must call
    /// [`AnalysisInFlight::finish`] with `cell` when done.
    Mine(AnalysisCell),
}

impl AnalysisInFlight {
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
        }
    }

    fn join_or_claim(&self, track_id: i64) -> Claim {
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(cell) = entries.get(&track_id).cloned() {
            drop(entries);
            let (lock, condvar) = &*cell;
            let mut guard = lock.lock().unwrap_or_else(PoisonError::into_inner);
            while guard.is_none() {
                guard = condvar.wait(guard).unwrap_or_else(PoisonError::into_inner);
            }
            return Claim::Done(guard.expect("the wait loop only exits once a result is set"));
        }
        let cell: AnalysisCell = Arc::new((Mutex::new(None), Condvar::new()));
        entries.insert(track_id, Arc::clone(&cell));
        Claim::Mine(cell)
    }

    fn finish(&self, track_id: i64, cell: &AnalysisCell, outcome: AndroidAnalysisOutcome) {
        {
            let (lock, condvar) = &**cell;
            *lock.lock().unwrap_or_else(PoisonError::into_inner) = Some(outcome);
            condvar.notify_all();
        }
        // Every waiter already holds its own clone of `cell`, so removing
        // the map entry here cannot lose a result — it only stops a *later*,
        // independent request for the same id from reading a stale outcome.
        self.entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&track_id);
    }

    /// True while `track_id` is claimed by an in-progress decode.
    pub(crate) fn contains(&self, track_id: i64) -> bool {
        self.entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains_key(&track_id)
    }
}

impl Default for AnalysisInFlight {
    fn default() -> Self {
        Self::new()
    }
}

/// Every handle one track's compute-on-missing decode needs. Built fresh
/// per call from `Arc`-backed fields, so it never borrows `MusicLibrary` for
/// the call's whole duration — the backfill worker thread builds its own
/// from its own clones of the same `Arc`s.
pub(crate) struct AnalysisContext<'a> {
    pub(crate) reader: &'a Mutex<Db>,
    pub(crate) writer: &'a Mutex<Db>,
    pub(crate) decoder: &'a Mutex<Option<Arc<dyn TrackPcmDecoder>>>,
    pub(crate) in_flight: &'a AnalysisInFlight,
    pub(crate) failed: &'a Mutex<HashSet<i64>>,
}

impl AnalysisContext<'_> {
    /// Computes and stores one track's analysis, deduplicating concurrent
    /// callers for the same id. `preempt` is the backfill to cancel if its
    /// current item is a different track (a foreground request only; the
    /// backfill's own worker passes `None` for its own items).
    pub(crate) fn compute(
        &self,
        track_id: i64,
        background: bool,
        preempt: Option<&TrackAnalysisBackfill>,
        current_slot: Option<&CurrentDecodeSlot>,
    ) -> Result<AndroidAnalysisOutcome, LibraryError> {
        if self.render_data_already_valid(track_id)? {
            return Ok(AndroidAnalysisOutcome::AlreadyImported);
        }
        match self.in_flight.join_or_claim(track_id) {
            Claim::Done(outcome) => Ok(outcome),
            Claim::Mine(cell) => {
                if let Some(backfill) = preempt {
                    backfill.preempt_current_unless(track_id);
                }
                let result = self.decode_one(track_id, background, current_slot);
                let outcome_for_waiters = *result
                    .as_ref()
                    .unwrap_or(&AndroidAnalysisOutcome::DecodeFailed);
                self.in_flight.finish(track_id, &cell, outcome_for_waiters);
                result
            }
        }
    }

    fn render_data_already_valid(&self, track_id: i64) -> Result<bool, LibraryError> {
        let reader = self.reader.lock().map_err(poisoned)?;
        let has_spectrogram = reprise_core::db::get_track_spectrogram(&reader, track_id)
            .map_err(database_error)?
            .is_some();
        let has_peaks = reprise_core::db::get_waveform_peaks(&reader, track_id)
            .map_err(database_error)?
            .is_some();
        Ok(has_spectrogram && has_peaks)
    }

    fn decode_one(
        &self,
        track_id: i64,
        background: bool,
        current_slot: Option<&CurrentDecodeSlot>,
    ) -> Result<AndroidAnalysisOutcome, LibraryError> {
        let decoder = self.decoder.lock().map_err(poisoned)?.clone();
        let Some(decoder) = decoder else {
            return Ok(AndroidAnalysisOutcome::NoDecoder);
        };

        // The URI and fingerprint are read under `reader` and the guard is
        // released before the decode call, per decision 6 of the mother
        // plan: the decoder callback never runs while `reader` or `writer`
        // is held.
        let (track_uri, fingerprint) = {
            let reader = self.reader.lock().map_err(poisoned)?;
            let track = reprise_core::queries::query_present_track_by_id(&reader, track_id)
                .map_err(query_error)?
                .ok_or(LibraryError::TrackNotFound { track_id })?;
            let fingerprint = reprise_core::db::track_source_fingerprint(&reader, track_id)
                .map_err(database_error)?
                .ok_or(LibraryError::TrackNotFound { track_id })?;
            (track.path, fingerprint)
        };

        let sink = AnalysisPcmSink::new();
        // `current_slot` gets the track id and the sink together, in one
        // write, right before the decode call starts: this is the only
        // point that publishes "this track is now decoding" to a foreground
        // preemption check (`TrackAnalysisBackfill::preempt_current_unless`),
        // so that check can never see a track id with no sink yet to cancel.
        if let Some(slot) = current_slot {
            *slot.lock().map_err(poisoned)? = Some((track_id, Arc::clone(&sink)));
        }
        let decode_result = decoder.decode(track_uri, Arc::clone(&sink), background);
        if let Some(slot) = current_slot {
            *slot.lock().map_err(poisoned)? = None;
        }

        // Cancellation is decided before anything else: a cancelled decoder
        // may still return `Ok(())` with a truncated stream, and a stream
        // this session never intended to finish must not be stored as if it
        // had (decision in A3 of the strand file).
        if sink.is_cancelled() {
            return Ok(AndroidAnalysisOutcome::Cancelled);
        }
        if let Err(error) = decode_result {
            tracing::debug!(track_id, %error, "track analysis decode failed");
            self.mark_failed(track_id)?;
            return Ok(AndroidAnalysisOutcome::DecodeFailed);
        }
        if let Some(reason) = sink.refused_reason() {
            tracing::debug!(track_id, reason, "track analysis session refused a chunk");
            self.mark_failed(track_id)?;
            return Ok(AndroidAnalysisOutcome::DecodeFailed);
        }
        let data = match sink.finish() {
            Ok(data) => data,
            Err(reason) => {
                tracing::debug!(track_id, reason, "track analysis produced no data");
                self.mark_failed(track_id)?;
                return Ok(AndroidAnalysisOutcome::DecodeFailed);
            }
        };

        let writer = self.writer.lock().map_err(poisoned)?;
        let outcome =
            reprise_core::db::set_track_render_data(&writer, track_id, fingerprint, &data)
                .map_err(database_error)?;
        drop(writer);
        match outcome {
            reprise_core::db::SpectrogramStoreOutcome::Stored => {
                Ok(AndroidAnalysisOutcome::Computed)
            }
            reprise_core::db::SpectrogramStoreOutcome::SourceChanged => {
                Ok(AndroidAnalysisOutcome::PhoneSourceChanged)
            }
        }
    }

    fn mark_failed(&self, track_id: i64) -> Result<(), LibraryError> {
        self.failed.lock().map_err(poisoned)?.insert(track_id);
        Ok(())
    }
}

fn poisoned<T>(_: std::sync::PoisonError<T>) -> LibraryError {
    LibraryError::Database {
        detail: "library handle poisoned by an earlier panic".to_owned(),
    }
}

fn database_error(error: impl std::fmt::Display) -> LibraryError {
    LibraryError::Database {
        detail: error.to_string(),
    }
}

fn query_error(error: impl std::fmt::Display) -> LibraryError {
    LibraryError::Query {
        detail: error.to_string(),
    }
}

#[uniffi::export]
impl MusicLibrary {
    /// Registers the platform decoder Kotlin implements. Called once, right
    /// after the library is opened (`SharedMusicLibrary.kt`).
    pub fn register_track_pcm_decoder(&self, decoder: Box<dyn TrackPcmDecoder>) {
        if let Ok(mut guard) = self.pcm_decoder.lock() {
            *guard = Some(Arc::from(decoder));
        }
    }
}

#[cfg(test)]
#[path = "compute_tests.rs"]
mod tests;
