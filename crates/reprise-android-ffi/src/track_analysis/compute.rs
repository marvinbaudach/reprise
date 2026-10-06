//! The sink Kotlin's decoder pushes PCM into, the `TrackPcmDecoder` callback
//! it implements, and the compute-on-missing path `mobile_sync.rs` calls
//! when a sidecar import ends in `Missing`/`Invalid` (decision 5 of the
//! mother plan).

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Condvar, Mutex, PoisonError};

use reprise_core::db::Db;
use reprise_core::render_data_session::{PartialRenderData, RenderDataSession};

use crate::track_analysis::decodes::{expected_frame_count, DecodeRegistry};
use crate::track_analysis::TrackAnalysisBackfill;
use crate::{LibraryError, MusicLibrary};

const MAX_FOREGROUND_COMPUTE_ROUNDS: usize = 3;

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

/// The sink still takes PCM.
const SINK_RUNNING: u8 = 0;
/// Told to stop by a foreground request preempting the backfill's item.
const SINK_CANCELLED: u8 = 1;
/// Told to stop because the track is no longer playing.
const SINK_SUPERSEDED: u8 = 2;

/// Owns one track's [`RenderDataSession`] for the duration of one decode
/// call. `stop_reason` is set from outside the decode call — by a foreground
/// request preempting the backfill's current item, or by a track change
/// superseding a foreground decode — so the decoder can be told to stop
/// without a second channel back into Kotlin. The first reason wins; it
/// decides whether waiters retry (`Cancelled`) or settle (`Superseded`).
#[derive(uniffi::Object)]
pub struct AnalysisPcmSink {
    session: Mutex<Option<RenderDataSession>>,
    stop_reason: AtomicU8,
    /// Set when a chunk was turned away because the sink had been told to
    /// stop: the decoder then returns with the stream cut short.
    cut_short: AtomicBool,
    /// Set when the session itself refused a chunk (a rate or channel
    /// change mid-stream): a data problem, distinct from the decoder giving
    /// up and distinct from being told to stop.
    refused: Mutex<Option<String>>,
}

impl AnalysisPcmSink {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            session: Mutex::new(Some(RenderDataSession::new())),
            stop_reason: AtomicU8::new(SINK_RUNNING),
            cut_short: AtomicBool::new(false),
            refused: Mutex::new(None),
        })
    }

    fn stop(&self, reason: u8) {
        let _ = self.stop_reason.compare_exchange(
            SINK_RUNNING,
            reason,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }

    pub(crate) fn cancel(&self) {
        self.stop(SINK_CANCELLED);
    }

    pub(super) fn supersede(&self) {
        self.stop(SINK_SUPERSEDED);
    }

    fn is_cancelled(&self) -> bool {
        self.stop_reason.load(Ordering::Acquire) != SINK_RUNNING
    }

    fn is_superseded(&self) -> bool {
        self.stop_reason.load(Ordering::Acquire) == SINK_SUPERSEDED
    }

    fn was_cut_short(&self) -> bool {
        self.cut_short.load(Ordering::Acquire)
    }

    /// What has been decoded so far. Only the copy of the decoded frames is
    /// taken under the session lock the decoder pushes through; the picture is
    /// built after it is released. `None` before one peak bucket is complete
    /// and once the session has been taken to finish.
    pub(super) fn partial(&self, expected_frames: usize) -> Option<PartialRenderData> {
        let source = self
            .session
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()?
            .partial_source();
        source.render(expected_frames)
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
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI cannot export borrowed byte slices"
    )]
    pub fn push_pcm_i16(&self, bytes: Vec<u8>, sample_rate_hz: u32, channel_count: u32) -> bool {
        if self.is_cancelled() {
            self.cut_short.store(true, Ordering::Release);
            return false;
        }
        let samples: Vec<i16> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| i16::from_le_bytes(*pair))
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
    /// The track stopped being the playing one and its foreground decode was
    /// stopped. Final for every waiter (no retry); nothing was stored and the
    /// track stays pending for the backfill.
    Superseded,
}

/// One pending or finished decode, shared between every caller waiting on
/// the same track id.
pub(crate) struct AnalysisCell {
    state: Mutex<CellState>,
    changed: Condvar,
}

struct CellState {
    outcome: Option<AndroidAnalysisOutcome>,
    /// How many supersedes naming another track have reached this decode. A
    /// caller compares it with the count it joined at: a supersede it was
    /// already waiting through is meant for it, one that came before it is
    /// not — that caller asked for the track again after it was left.
    supersedes: u64,
}

type SharedAnalysisCell = Arc<AnalysisCell>;

impl AnalysisCell {
    fn new() -> SharedAnalysisCell {
        Arc::new(Self {
            state: Mutex::new(CellState {
                outcome: None,
                supersedes: 0,
            }),
            changed: Condvar::new(),
        })
    }

    fn state(&self) -> std::sync::MutexGuard<'_, CellState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Waits for the decode's outcome as a caller that did not start it. A
    /// foreground waiter is let go with `Superseded` as soon as a supersede
    /// reaches the decode, even a backfill decode that carries on and
    /// stores: the waiter's track is no longer playing, and it must not hold
    /// the foreground import lane until a decode it no longer needs ends.
    fn wait(&self, background: bool) -> Claim {
        let mut state = self.state();
        let joined_at = state.supersedes;
        loop {
            if let Some(outcome) = state.outcome {
                let superseded_before_joining =
                    outcome == AndroidAnalysisOutcome::Superseded && state.supersedes == joined_at;
                return if superseded_before_joining {
                    Claim::Stale
                } else {
                    Claim::Done(outcome)
                };
            }
            if !background && state.supersedes != joined_at {
                return Claim::Done(AndroidAnalysisOutcome::Superseded);
            }
            state = self
                .changed
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    fn note_supersede(&self) {
        self.state().supersedes += 1;
        self.changed.notify_all();
    }

    /// True once any supersede naming another track has reached this decode.
    fn was_superseded(&self) -> bool {
        self.state().supersedes > 0
    }

    fn settle(&self, outcome: AndroidAnalysisOutcome) {
        self.state().outcome = Some(outcome);
        self.changed.notify_all();
    }
}

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
    entries: Mutex<HashMap<i64, SharedAnalysisCell>>,
    decodes: DecodeRegistry,
}

pub(crate) enum Claim {
    /// Another caller already finished (or finishes while this one waits).
    Done(AndroidAnalysisOutcome),
    /// The joined decode was superseded before this caller arrived, so its
    /// `Superseded` is not this caller's answer: the track is wanted again.
    Stale,
    /// This caller now owns the decode; it must call
    /// [`AnalysisInFlight::finish`] with `cell` when done.
    Mine(SharedAnalysisCell),
}

impl AnalysisInFlight {
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            decodes: DecodeRegistry::default(),
        }
    }

    /// The decodes running right now, with their live sinks.
    pub(crate) fn decodes(&self) -> &DecodeRegistry {
        &self.decodes
    }

    fn join_or_claim(&self, track_id: i64, background: bool) -> Claim {
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(cell) = entries.get(&track_id).cloned() {
            drop(entries);
            return cell.wait(background);
        }
        let cell = AnalysisCell::new();
        entries.insert(track_id, Arc::clone(&cell));
        Claim::Mine(cell)
    }

    fn finish(&self, track_id: i64, cell: &AnalysisCell, outcome: AndroidAnalysisOutcome) {
        // Every waiter already holds its own clone of `cell`. Retire the map
        // entry before waking them so a foreground retry after `Cancelled`
        // cannot immediately join the same completed cell again.
        self.entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&track_id);
        cell.settle(outcome);
    }

    /// Stops the foreground decode of every track but `keep` and lets go of
    /// every foreground caller waiting on any other track, the backfill's
    /// decodes included (those carry on and store). Non-blocking: it only
    /// flips flags. The cells are marked before the sinks, so a decode that
    /// registers its sink in between still finds the mark (`decode_one`).
    pub(crate) fn supersede_except(&self, keep: Option<i64>) {
        {
            let entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
            for (track_id, cell) in entries.iter() {
                if Some(*track_id) != keep {
                    cell.note_supersede();
                }
            }
        }
        self.decodes.supersede_foreground_except(keep);
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
    /// callers for the same id. A foreground caller retries an inherited
    /// background cancellation for at most three rounds; background callers
    /// make one attempt. `preempt` is the backfill to cancel if its current
    /// item is a different track (a foreground request only; the backfill's
    /// own worker passes `None` for its own items).
    pub(crate) fn compute(
        &self,
        track_id: i64,
        background: bool,
        preempt: Option<&TrackAnalysisBackfill>,
        current_slot: Option<&CurrentDecodeSlot>,
    ) -> Result<AndroidAnalysisOutcome, LibraryError> {
        let rounds = if background {
            1
        } else {
            MAX_FOREGROUND_COMPUTE_ROUNDS
        };
        for round in 0..rounds {
            if self.render_data_already_valid(track_id)? {
                return Ok(AndroidAnalysisOutcome::AlreadyImported);
            }
            match self.in_flight.join_or_claim(track_id, background) {
                Claim::Done(AndroidAnalysisOutcome::Cancelled) | Claim::Stale
                    if !background && round + 1 < rounds =>
                {
                    continue;
                }
                Claim::Stale => return Ok(AndroidAnalysisOutcome::Superseded),
                Claim::Done(outcome) => return Ok(outcome),
                Claim::Mine(cell) => {
                    if let Some(backfill) = preempt {
                        backfill.preempt_current_unless(track_id);
                    }
                    let result = self.decode_one(track_id, background, current_slot, &cell);
                    let outcome_for_waiters = *result
                        .as_ref()
                        .unwrap_or(&AndroidAnalysisOutcome::DecodeFailed);
                    self.in_flight.finish(track_id, &cell, outcome_for_waiters);
                    return result;
                }
            }
        }
        unreachable!("the compute loop always returns on its final round")
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
        cell: &AnalysisCell,
    ) -> Result<AndroidAnalysisOutcome, LibraryError> {
        let decoder = self.decoder.lock().map_err(poisoned)?.clone();
        let Some(decoder) = decoder else {
            return Ok(AndroidAnalysisOutcome::NoDecoder);
        };

        // The URI and fingerprint are read under `reader` and the guard is
        // released before the decode call, per decision 6 of the mother
        // plan: the decoder callback never runs while `reader` or `writer`
        // is held.
        let (track_uri, fingerprint, expected_frames) = {
            let reader = self.reader.lock().map_err(poisoned)?;
            let track = reprise_core::queries::query_present_track_by_id(&reader, track_id)
                .map_err(query_error)?
                .ok_or(LibraryError::TrackNotFound { track_id })?;
            // A track cut from a CUE file is a stretch of its file, and this decode
            // measures the whole file. Storing that under the track would be wrong
            // for good: the stored fingerprint is the file's, so nothing would ever
            // measure the track again. The phone cuts tracks out of the decode in a
            // later change; until then such a track has no analysis.
            if track.segment.is_some() {
                return Ok(AndroidAnalysisOutcome::DecodeFailed);
            }
            let fingerprint = reprise_core::db::track_source_fingerprint(&reader, track_id)
                .map_err(database_error)?
                .ok_or(LibraryError::TrackNotFound { track_id })?;
            (
                track.path,
                fingerprint,
                expected_frame_count(track.duration_ms),
            )
        };

        let sink = AnalysisPcmSink::new();
        // Registered for the whole call, store included, and dropped on every
        // way out of this function.
        let _registration =
            self.in_flight
                .decodes()
                .register(track_id, &sink, expected_frames, background);
        // A supersede that came after this caller's claim but before the sink
        // was registered marked the cell and found no sink to stop.
        if !background && cell.was_superseded() {
            sink.supersede();
        }
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
        // had (decision in A3 of the strand file). The one exception is a
        // supersede that came after the decoder reached the end of the
        // stream: nothing was turned away, so the data is whole and its cost
        // already paid, and it is stored like any finished decode.
        let superseded_after_the_end =
            sink.is_superseded() && decode_result.is_ok() && !sink.was_cut_short();
        if sink.is_cancelled() && !superseded_after_the_end {
            return Ok(if sink.is_superseded() {
                AndroidAnalysisOutcome::Superseded
            } else {
                AndroidAnalysisOutcome::Cancelled
            });
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

#[cfg(test)]
#[path = "compute_retry_tests.rs"]
mod retry_tests;

#[cfg(test)]
#[path = "compute_progress_tests.rs"]
mod progress_tests;

#[cfg(test)]
#[path = "compute_supersede_tests.rs"]
mod supersede_tests;

#[cfg(test)]
#[path = "compute_supersede_race_tests.rs"]
mod supersede_race_tests;
