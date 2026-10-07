//! The sink Kotlin's decoder pushes PCM into, the `TrackPcmDecoder` callback
//! it implements, and the compute-on-missing path `mobile_sync.rs` calls
//! when a sidecar import ends in `Missing`/`Invalid` (decision 5 of the
//! mother plan).

use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex, PoisonError};

use reprise_core::db::Db;

use crate::track_analysis::decodes::{expected_frame_count, DecodeRegistry};
use crate::track_analysis::segment_job::SegmentJob;
pub(crate) use crate::track_analysis::sink::AnalysisPcmSink;
use crate::track_analysis::sink::FinishedAnalysis;
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
    /// The track stopped being the playing one. Final for the caller that
    /// receives it (no retry). The decode it was waiting on either stopped
    /// before the end of the stream, storing nothing and leaving the track
    /// pending for the backfill, or carries on: a backfill decode, or one that
    /// had already decoded the whole stream, is still stored, and its owner
    /// returns `Computed` while the foreground waiters it let go get this.
    /// A caller that joined after the supersede is not given it (it retries).
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
    ///
    /// `joined_at` is the supersede count read when the caller joined, while
    /// the in-flight map was still locked (`AnalysisInFlight::join`).
    fn wait(&self, joined_at: u64, background: bool) -> Claim {
        let mut state = self.state();
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

/// A caller that found the track already being decoded, with the number of
/// supersedes that had reached the decode before it joined.
pub(crate) struct Waiter {
    cell: SharedAnalysisCell,
    joined_at: u64,
}

impl Waiter {
    pub(crate) fn wait(&self, background: bool) -> Claim {
        self.cell.wait(self.joined_at, background)
    }
}

pub(crate) enum Join {
    Waiting(Waiter),
    Mine(SharedAnalysisCell),
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
        match self.join(track_id) {
            Join::Waiting(waiter) => waiter.wait(background),
            Join::Mine(cell) => Claim::Mine(cell),
        }
    }

    /// Joins the decode of `track_id` already running, or claims it.
    ///
    /// A joiner reads the decode's supersede count before the map lock is
    /// released. `supersede_except` counts under that same lock (map, then
    /// cell, the order used here too), so every supersede is either one this
    /// caller joined after or one it waits through — never one that slipped in
    /// between and is miscounted as older than the caller.
    pub(crate) fn join(&self, track_id: i64) -> Join {
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(cell) = entries.get(&track_id).cloned() {
            let joined_at = cell.state().supersedes;
            drop(entries);
            return Join::Waiting(Waiter { cell, joined_at });
        }
        let cell = AnalysisCell::new();
        entries.insert(track_id, Arc::clone(&cell));
        Join::Mine(cell)
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
}

impl AnalysisContext<'_> {
    /// Computes and stores one track's analysis, deduplicating concurrent
    /// callers for the same id. A foreground caller retries for at most three
    /// rounds when the decode it joined was cancelled (a backfill preemption)
    /// or superseded before it joined (`Claim::Stale`: the supersede was meant
    /// for an earlier caller); background callers make one attempt. A `Stale`
    /// on the last round ends as `Cancelled`, retryable, because this caller
    /// was never superseded. `preempt` is the backfill to cancel if its current
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
                // Out of rounds on supersedes meant for earlier callers: nobody
                // superseded this one, so it ends retryable, never final.
                Claim::Stale => return Ok(AndroidAnalysisOutcome::Cancelled),
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

    pub(super) fn render_data_already_valid(&self, track_id: i64) -> Result<bool, LibraryError> {
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
        // A track cut from a CUE file is measured from its own stretch of the
        // file, together with the other tracks of the file still lacking
        // their analysis (`segment_job.rs`); storing the whole file's data
        // under it would be wrong for good, since the fingerprint is the
        // file's and nothing would measure the track again.
        let (track_uri, fingerprint, expected_frames, job) = {
            let reader = self.reader.lock().map_err(poisoned)?;
            let track = reprise_core::queries::query_present_track_by_id(&reader, track_id)
                .map_err(query_error)?
                .ok_or(LibraryError::TrackNotFound { track_id })?;
            let job = SegmentJob::plan(&reader, &track)?;
            let fingerprint = reprise_core::db::track_source_fingerprint(&reader, track_id)
                .map_err(database_error)?
                .ok_or(LibraryError::TrackNotFound { track_id })?;
            (
                track.path,
                fingerprint,
                expected_frame_count(track.duration_ms),
                job,
            )
        };

        let sink = job.as_ref().map_or_else(AnalysisPcmSink::new, |job| {
            AnalysisPcmSink::segmented(&job.bounds())
        });
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
        // A file that cannot be decoded, or whose rate or channel count
        // changes mid-stream, fails for every track the decode measured.
        let measured: Vec<i64> = job
            .as_ref()
            .map_or_else(|| vec![track_id], |job| job.track_ids().collect());
        if let Err(error) = decode_result {
            tracing::debug!(track_id, %error, "track analysis decode failed");
            self.mark_failed(&measured, &error.to_string())?;
            return Ok(AndroidAnalysisOutcome::DecodeFailed);
        }
        if let Some(reason) = sink.refused_reason() {
            tracing::debug!(track_id, reason, "track analysis session refused a chunk");
            self.mark_failed(&measured, &reason)?;
            return Ok(AndroidAnalysisOutcome::DecodeFailed);
        }
        let data = match (sink.finish(), job) {
            (FinishedAnalysis::Whole(Ok(data)), _) => data,
            (FinishedAnalysis::Segmented(results), Some(job)) => {
                let writer = self.writer.lock().map_err(poisoned)?;
                return job.store(&writer, fingerprint, results);
            }
            (FinishedAnalysis::Whole(Err(reason)), _) => {
                tracing::debug!(track_id, reason, "track analysis produced no data");
                self.mark_failed(&measured, &reason)?;
                return Ok(AndroidAnalysisOutcome::DecodeFailed);
            }
            (FinishedAnalysis::Segmented(_), None) => {
                unreachable!("only a CUE track's job makes a segmented sink")
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

    /// Remembers in the library that `track_ids` could not be measured, so
    /// the backfill leaves them alone until their file changes (finding C8).
    /// Called only after the decoder has returned, never while it runs.
    fn mark_failed(&self, track_ids: &[i64], reason: &str) -> Result<(), LibraryError> {
        let writer = self.writer.lock().map_err(poisoned)?;
        for track_id in track_ids {
            reprise_core::spectrogram_backfill::record_render_data_failure(
                &writer, *track_id, reason,
            )
            .map_err(database_error)?;
        }
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

#[cfg(test)]
#[path = "compute_cue_tests.rs"]
mod cue_tests;
