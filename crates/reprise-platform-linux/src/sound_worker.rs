//! Explicit Linux worker for the Sound tab's per-track snapshot.
//!
//! Like `spectrogram_backfill`, the thread and its channels live here while the
//! work itself stays pure in `reprise_core::sound_snapshot`: a frontend asks for
//! a track and renders the answers it gets back, and never opens a database or
//! ranks anything on its own thread (`SIM-4`, `SIM-6`).

use std::path::{Path, PathBuf};
use std::time::Duration;

use reprise_core::db::Db;
use reprise_core::sound_snapshot::{
    sound_snapshot, ProgressWatch, SoundSnapshot, SoundSnapshotOptions,
};
use reprise_core::sound_stats::SoundStatsCache;

/// How long the worker waits before reading the profile inventory again while a
/// track has no profile yet.
const PROGRESS_RECHECK: Duration = Duration::from_millis(500);

/// One asked-for track. `generation` is an opaque caller token: the worker only
/// carries it back on the matching response, so the caller can tell an answer
/// for the current track from a late one for a superseded request.
#[derive(Debug, Clone, Copy)]
pub struct SoundRequest {
    pub generation: u64,
    pub track_id: i64,
    pub options: SoundSnapshotOptions,
}

#[derive(Debug, Clone)]
pub struct SoundResponse {
    pub generation: u64,
    pub snapshot: SoundSnapshot,
}

/// A running sound worker. Dropping the handle closes the request channel,
/// which ends the thread — that is how switching the module off stops all sound
/// work.
pub struct SoundWorkerHandle {
    requests: async_channel::Sender<SoundRequest>,
    responses: async_channel::Receiver<SoundResponse>,
}

impl SoundWorkerHandle {
    /// Starts the worker on its own database handle.
    ///
    /// A thread the system refuses to start degrades to `None` — the caller then
    /// shows no sound results, exactly as for a library without a path. This
    /// runs while a window is being built and must not take it down.
    pub fn start(database_path: PathBuf) -> Option<Self> {
        let (requests, request_receiver) = async_channel::unbounded::<SoundRequest>();
        let (response_sender, responses) = async_channel::unbounded();
        if let Err(error) = std::thread::Builder::new()
            .name("reprise-sound-panel".into())
            .spawn(move || worker_loop(&database_path, &request_receiver, &response_sender))
        {
            tracing::warn!(%error, "could not start sound-panel worker");
            return None;
        }
        Some(Self {
            requests,
            responses,
        })
    }

    /// Asks for one track. A request the worker cannot take is logged and
    /// dropped: the caller keeps its current view rather than waiting forever.
    pub fn request(&self, request: SoundRequest) {
        if let Err(error) = self.requests.try_send(request) {
            tracing::warn!(%error, "sound-panel request dropped");
        }
    }

    /// The stream of answers, in the order the worker produced them.
    pub fn responses(&self) -> async_channel::Receiver<SoundResponse> {
        self.responses.clone()
    }
}

fn worker_loop(
    path: &Path,
    requests: &async_channel::Receiver<SoundRequest>,
    responses: &async_channel::Sender<SoundResponse>,
) {
    let db = match Db::open_ready(path) {
        Ok(db) => db,
        Err(error) => {
            tracing::warn!(%error, "sound-panel worker could not open library");
            report_open_failure(&error.to_string(), requests, responses);
            return;
        }
    };
    let mut stats_cache = SoundStatsCache::default();
    while let Ok(mut request) = requests.recv_blocking() {
        let mut watch = ProgressWatch::default();
        loop {
            let mut newer = false;
            loop {
                match requests.try_recv() {
                    Ok(request_from_caller) => {
                        request = request_from_caller;
                        newer = true;
                    }
                    Err(async_channel::TryRecvError::Empty) => break,
                    // The caller dropped its handle: the module is off.
                    Err(async_channel::TryRecvError::Closed) => return,
                }
            }
            if newer {
                watch = ProgressWatch::default();
            }
            let snapshot = sound_snapshot(&db, &mut stats_cache, request.track_id, request.options);
            let inventory = match &snapshot {
                SoundSnapshot::Progress { ready, total } => Some((*ready, *total)),
                _ => None,
            };
            let next_watch = inventory.and_then(|inventory| watch.observe(inventory));
            let settled = inventory.is_some() && next_watch.is_none();
            if responses
                .send_blocking(SoundResponse {
                    generation: request.generation,
                    snapshot: if settled {
                        SoundSnapshot::Unavailable
                    } else {
                        snapshot
                    },
                })
                .is_err()
            {
                return;
            }
            let Some(next_watch) = next_watch else {
                break;
            };
            watch = next_watch;
            std::thread::sleep(PROGRESS_RECHECK);
        }
    }
}

/// Answers every request with the failure the caller could not see otherwise:
/// without this the response channel just closes and the tab keeps showing an
/// empty progress bar for the whole session.
fn report_open_failure(
    message: &str,
    requests: &async_channel::Receiver<SoundRequest>,
    responses: &async_channel::Sender<SoundResponse>,
) {
    while let Ok(request) = requests.recv_blocking() {
        if responses
            .send_blocking(SoundResponse {
                generation: request.generation,
                snapshot: SoundSnapshot::Error(message.to_owned()),
            })
            .is_err()
        {
            return;
        }
    }
}
