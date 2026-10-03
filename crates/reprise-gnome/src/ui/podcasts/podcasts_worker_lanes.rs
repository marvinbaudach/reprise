//! Routing and lifetime management for the independent podcast worker lanes.

use std::path::PathBuf;
use std::sync::Arc;

use reprise_core::db::{Db, DbError};

use super::{PodcastsOperation, QueuedRequest};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PodcastsLane {
    Feeds,
    Downloads,
}

impl PodcastsLane {
    pub(super) const fn name(self) -> &'static str {
        match self {
            Self::Feeds => "feeds",
            Self::Downloads => "downloads",
        }
    }

    const fn thread_name(self) -> &'static str {
        match self {
            Self::Feeds => "reprise-podcast-feeds",
            Self::Downloads => "reprise-podcast-downloads",
        }
    }
}

pub(super) const fn lane_for(operation: &PodcastsOperation) -> PodcastsLane {
    match operation {
        PodcastsOperation::Refresh { .. }
        | PodcastsOperation::LoadMore { .. }
        | PodcastsOperation::SyncSubscription { .. } => PodcastsLane::Feeds,
        PodcastsOperation::Download { .. } | PodcastsOperation::FillDownloads => {
            PodcastsLane::Downloads
        }
    }
}

pub(super) type LaneExecutor =
    Arc<dyn Fn(Option<&Result<Db, DbError>>, &QueuedRequest) + Send + Sync>;

/// Spawns one lane thread with one connection that it reuses for its lifetime.
///
/// Keeping feed work on the same long-lived connection preserves the
/// connection-address-keyed retry state while the downloads lane runs independently.
pub(super) fn spawn_lane(
    lane: PodcastsLane,
    database_path: Option<PathBuf>,
    executor: LaneExecutor,
) -> async_channel::Sender<QueuedRequest> {
    let (sender, receiver) = async_channel::unbounded::<QueuedRequest>();
    let result = std::thread::Builder::new()
        .name(lane.thread_name().into())
        .spawn(move || {
            let connection = database_path
                .as_deref()
                .map(|path| Db::open_migrated(Some(path)));
            while let Ok(queued) = receiver.recv_blocking() {
                tracing::info!(
                    lane = lane.name(),
                    generation = queued.request.generation,
                    operation = ?queued.request.operation,
                    queued_ms = queued.queued_at.elapsed().as_millis() as u64,
                    "podcast worker dequeued request"
                );
                executor(connection.as_ref(), &queued);
            }
        });
    if let Err(error) = result {
        tracing::warn!(lane = lane.name(), %error, "could not start podcast worker lane");
    }
    sender
}
