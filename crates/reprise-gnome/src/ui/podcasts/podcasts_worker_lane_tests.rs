use super::*;

use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use reprise_core::podcasts::download_state::DownloadState;

const RESPONSE_DEADLINE: Duration = Duration::from_secs(10);
const FILL_GATE_DEADLINE: Duration = Duration::from_secs(15);

struct ReleaseGate {
    gate: Arc<(Mutex<bool>, Condvar)>,
}

impl Drop for ReleaseGate {
    fn drop(&mut self) {
        let (released, changed) = &*self.gate;
        *released
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = true;
        changed.notify_all();
    }
}

fn response_within(
    receiver: async_channel::Receiver<PodcastsResponse>,
    deadline: Duration,
    failure: &'static str,
) -> PodcastsResponse {
    let (sender, response) = std::sync::mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let _ = sender.send(receiver.recv_blocking());
    });
    response
        .recv_timeout(deadline)
        .expect(failure)
        .expect("podcast response channel should remain open")
}

fn request(
    operation: PodcastsOperation,
) -> (PodcastsRequest, async_channel::Receiver<PodcastsResponse>) {
    let (response, receiver) = podcasts_response_channel();
    (
        PodcastsRequest {
            generation: 1,
            operation,
            response,
        },
        receiver,
    )
}

#[test]
fn pod_28_a_refresh_completes_while_a_fill_up_is_still_downloading() {
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let release = ReleaseGate {
        gate: Arc::clone(&gate),
    };
    let (fill_started, started) = std::sync::mpsc::sync_channel(1);
    let executor: LaneExecutor = Arc::new(move |_, queued| match &queued.request.operation {
        PodcastsOperation::FillDownloads => {
            fill_started.send(()).unwrap();
            let (released, changed) = &*gate;
            let mut released = released.lock().unwrap();
            while !*released {
                let (next, timeout) = changed.wait_timeout(released, FILL_GATE_DEADLINE).unwrap();
                assert!(!timeout.timed_out(), "fill-up release gate timed out");
                released = next;
            }
            send_response(
                &queued.request,
                Ok(PodcastsWorkerResult::Filled(Default::default())),
            );
        }
        PodcastsOperation::Refresh { .. } => send_response(
            &queued.request,
            Ok(PodcastsWorkerResult::Refreshed(Default::default())),
        ),
        operation => panic!("unexpected operation in lane test: {operation:?}"),
    });
    let runtime = PodcastsRuntime::new_for_test(true, executor);

    let (fill, fill_response) = request(PodcastsOperation::FillDownloads);
    assert!(runtime.request(fill));
    started
        .recv_timeout(RESPONSE_DEADLINE)
        .expect("fill-up should start before the deadline");

    let (refresh, refresh_response) = request(PodcastsOperation::Refresh {
        policy: podcasts::refresh::RefreshPolicy::Force,
        kind: None,
    });
    assert!(runtime.request(refresh));
    let refreshed = response_within(
        refresh_response,
        RESPONSE_DEADLINE,
        "head-of-line blocking: refresh waited behind a running fill-up",
    );
    assert!(matches!(
        refreshed.result,
        Ok(PodcastsWorkerResult::Refreshed(_))
    ));

    drop(release);
    let filled = response_within(
        fill_response,
        RESPONSE_DEADLINE,
        "released fill-up should finish before the deadline",
    );
    assert!(matches!(filled.result, Ok(PodcastsWorkerResult::Filled(_))));
}

#[test]
fn pod_28_downloads_and_feed_work_take_separate_lanes() {
    assert_eq!(
        lane_for(&PodcastsOperation::Refresh {
            policy: podcasts::refresh::RefreshPolicy::Due,
            kind: None,
        }),
        PodcastsLane::Feeds
    );
    assert_eq!(
        lane_for(&PodcastsOperation::LoadMore {
            subscription_id: 1,
            end: 30,
        }),
        PodcastsLane::Feeds
    );
    assert_eq!(
        lane_for(&PodcastsOperation::SyncSubscription {
            subscription_id: 1,
            abort: podcasts::pipeline::SyncAbort::new(),
        }),
        PodcastsLane::Feeds
    );
    assert_eq!(
        lane_for(&PodcastsOperation::Download { episode_id: 1 }),
        PodcastsLane::Downloads
    );
    assert_eq!(
        lane_for(&PodcastsOperation::FillDownloads),
        PodcastsLane::Downloads
    );
}

#[test]
fn download_job_counts_only_terminal_published_states() {
    let states = [
        DownloadState::Queued,
        DownloadState::Downloading {
            received_bytes: 5,
            total_bytes: Some(10),
        },
        DownloadState::Downloaded { bytes: 10 },
        DownloadState::Failed {
            message: "offline".to_owned(),
        },
    ];

    assert_eq!(
        count_download_states(&states),
        DownloadJobCounts {
            downloaded: 1,
            failed: 1,
        }
    );
}
