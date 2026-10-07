//! The sink Kotlin's decoder pushes PCM into: one decode call's analysis
//! session, told to stop from outside the call.
//!
//! A whole-file track is measured by one [`RenderDataSession`]. A track cut
//! from a CUE file is measured with the other tracks of its file that still
//! need it, in one [`SegmentedRenderDataSession`] that places each chunk by
//! the decoder's timestamp (finding C7).

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use reprise_core::render_data_segments::{SegmentBounds, SegmentedRenderDataSession};
use reprise_core::render_data_session::{
    PartialRenderData, PartialSource, RenderDataSession, RenderDataSessionError,
};
use reprise_core::waveform::TrackRenderData;

/// The sink still takes PCM.
const SINK_RUNNING: u8 = 0;
/// Told to stop by a foreground request preempting the backfill's item.
const SINK_CANCELLED: u8 = 1;
/// Told to stop because the track is no longer playing.
const SINK_SUPERSEDED: u8 = 2;

/// What one decode call measures. A whole-file session is boxed: it carries
/// its analysis state inline and dwarfs the segmented variant's vector.
enum SinkSession {
    Whole(Box<RenderDataSession>),
    /// The tracks of one CUE file. The first is the one asked for: its
    /// partial picture is the one a progress read sees.
    Segmented(SegmentedRenderDataSession),
}

/// What a finished decode measured: one result for a whole file, one per
/// segment, in the order given, for a CUE file.
pub(crate) enum FinishedAnalysis {
    Whole(Result<TrackRenderData, String>),
    Segmented(Vec<Result<TrackRenderData, String>>),
}

/// Owns one decode call's session. `stop_reason` is set from outside the
/// decode call — by a foreground request preempting the backfill's current
/// item, or by a track change superseding a foreground decode — so the decoder
/// can be told to stop without a second channel back into Kotlin. The first
/// reason wins; it decides whether waiters retry (`Cancelled`) or stop asking
/// (`Superseded`). A supersede that arrives after the whole stream was pushed
/// discards nothing: the decode is stored as if it had not come (`decode_one`).
#[derive(uniffi::Object)]
pub struct AnalysisPcmSink {
    session: Mutex<Option<SinkSession>>,
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
    fn with_session(session: SinkSession) -> Arc<Self> {
        Arc::new(Self {
            session: Mutex::new(Some(session)),
            stop_reason: AtomicU8::new(SINK_RUNNING),
            cut_short: AtomicBool::new(false),
            refused: Mutex::new(None),
        })
    }

    /// A sink measuring one whole file.
    pub(super) fn new() -> Arc<Self> {
        Self::with_session(SinkSession::Whole(Box::new(RenderDataSession::new())))
    }

    /// A sink measuring the stretches `segments` of one file, each as a
    /// whole-file decode of that stretch would.
    pub(super) fn segmented(segments: &[SegmentBounds]) -> Arc<Self> {
        Self::with_session(SinkSession::Segmented(
            SegmentedRenderDataSession::with_sessions(segments, RenderDataSession::new),
        ))
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

    pub(super) fn is_cancelled(&self) -> bool {
        self.stop_reason.load(Ordering::Acquire) != SINK_RUNNING
    }

    pub(super) fn is_superseded(&self) -> bool {
        self.stop_reason.load(Ordering::Acquire) == SINK_SUPERSEDED
    }

    pub(super) fn was_cut_short(&self) -> bool {
        self.cut_short.load(Ordering::Acquire)
    }

    /// What has been decoded so far of the track asked for — for a CUE file,
    /// of its own stretch only. Only the copy of the decoded frames is taken
    /// under the session lock the decoder pushes through; the picture is built
    /// after it is released. `None` before one peak bucket is complete and
    /// once the session has been taken to finish.
    pub(super) fn partial(&self, expected_frames: usize) -> Option<PartialRenderData> {
        let source: PartialSource = match self
            .session
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()?
        {
            SinkSession::Whole(session) => session.partial_source(),
            SinkSession::Segmented(session) => session.partial_source(0)?,
        };
        source.render(expected_frames)
    }

    pub(super) fn refused_reason(&self) -> Option<String> {
        self.refused
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Takes the session out and finishes it. Only ever called once, after
    /// the decode call has returned and cancellation has been ruled out.
    pub(super) fn finish(&self) -> FinishedAnalysis {
        let session = self
            .session
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
            .expect("finish is called at most once per sink");
        match session {
            SinkSession::Whole(session) => {
                FinishedAnalysis::Whole(session.finish().map_err(|error| error.to_string()))
            }
            SinkSession::Segmented(session) => FinishedAnalysis::Segmented(
                session
                    .finish()
                    .into_iter()
                    .map(|result| result.map_err(|error| error.to_string()))
                    .collect(),
            ),
        }
    }

    fn push(
        &self,
        bytes: &[u8],
        sample_rate_hz: u32,
        channel_count: u32,
        start_us: Option<i64>,
    ) -> bool {
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
        let pushed: Result<(), RenderDataSessionError> = match session {
            SinkSession::Whole(session) => {
                session.push_pcm_i16(&samples, sample_rate_hz, channel_count)
            }
            SinkSession::Segmented(session) => {
                session.push_pcm_i16(&samples, sample_rate_hz, channel_count, start_us)
            }
        };
        match pushed {
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

#[uniffi::export]
impl AnalysisPcmSink {
    /// `false` tells the decoder to stop: either cancelled, or the session
    /// refused this chunk (a rate or channel change mid-stream). A chunk
    /// pushed without its time continues from the running frame count.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI cannot export borrowed byte slices"
    )]
    pub fn push_pcm_i16(&self, bytes: Vec<u8>, sample_rate_hz: u32, channel_count: u32) -> bool {
        self.push(&bytes, sample_rate_hz, channel_count, None)
    }

    /// [`push_pcm_i16`](Self::push_pcm_i16) for a chunk that starts
    /// `presentation_time_us` microseconds into the file, as the decoder
    /// reports it. The tracks of a CUE file are cut by that time, so a chunk
    /// the decoder dropped does not shift the tracks after it.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI cannot export borrowed byte slices"
    )]
    pub fn push_pcm_i16_at(
        &self,
        bytes: Vec<u8>,
        sample_rate_hz: u32,
        channel_count: u32,
        presentation_time_us: i64,
    ) -> bool {
        self.push(
            &bytes,
            sample_rate_hz,
            channel_count,
            Some(presentation_time_us),
        )
    }
}
