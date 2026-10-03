use std::cell::Cell;
use std::rc::Rc;

use reprise_core::connectivity::Connectivity;

use super::ConnectivityTargets;
use crate::ui::cover_download_batch::{BatchProgress, BatchState, CoverDownloadBatch};
use crate::ui::cover_download_worker::{CoverDownloadRuntime, DownloadOutcome, DownloadRequest};

struct BatchFixture {
    batch: Rc<CoverDownloadBatch>,
    targets: ConnectivityTargets,
    requests: async_channel::Receiver<DownloadRequest>,
    root: tempfile::TempDir,
}

fn batch_fixture() -> BatchFixture {
    gtk4::init().expect("GTK test display");
    let root = tempfile::tempdir().unwrap();
    let track = root.path().join("waiting.flac");
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../reprise-core/tests/fixtures/sine.flac");
    std::fs::copy(source, &track).unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    crate::test_db::connection(&conn)
        .execute(
            "INSERT INTO tracks (path, title, added_at) VALUES (?1, 'Waiting', 1)",
            [track.to_string_lossy().as_ref()],
        )
        .unwrap();
    let (worker, requests) = async_channel::unbounded();
    let runtime = CoverDownloadRuntime {
        enabled: Rc::new(Cell::new(true)),
        worker,
    };
    let track_list = Rc::new(crate::ui::track_list::TrackList::new(
        conn.clone(),
        Box::new(|_, _, _, _| {}),
        |_, _, _, _| {},
        crate::ui::track_list::queue_sections::QueueViewModel::default,
        runtime.clone(),
    ));
    let batch = CoverDownloadBatch::new(&conn, &runtime, &track_list, None);
    let targets = ConnectivityTargets::for_test(&batch);
    BatchFixture {
        batch,
        targets,
        requests,
        root,
    }
}

fn take_request(requests: &async_channel::Receiver<DownloadRequest>) -> DownloadRequest {
    crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
        !requests.is_empty()
    });
    requests
        .try_recv()
        .expect("cover pass dispatches a request")
}

fn finish(fixture: &BatchFixture, request: &DownloadRequest, outcome: DownloadOutcome) {
    request.response.try_send(outcome).unwrap();
    crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
        !fixture.batch.running_for_test()
    });
}

fn project_return(targets: &ConnectivityTargets) {
    targets.project(Connectivity::Offline);
    targets.project(Connectivity::Online);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn net_7c_a_transient_failure_retries_exactly_once_on_the_next_return() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let fixture = batch_fixture();
    fixture.batch.start_user_triggered();
    let first = take_request(&fixture.requests);
    finish(&fixture, &first, DownloadOutcome::TransientFailure);
    let generation = fixture.batch.generation_for_test();

    project_return(&fixture.targets);
    let retry = take_request(&fixture.requests);
    assert_eq!(fixture.batch.generation_for_test(), generation + 1);

    fixture.targets.project(Connectivity::Online);
    assert_eq!(fixture.batch.generation_for_test(), generation + 1);
    finish(
        &fixture,
        &retry,
        DownloadOutcome::Downloaded(fixture.root.path().join("cover.jpg")),
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn net_7c_a_return_after_a_clean_pass_starts_nothing_and_reports_no_progress() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let fixture = batch_fixture();
    let progress_events = Rc::new(Cell::new(0));
    fixture.batch.subscribe_progress(|| true, {
        let progress_events = progress_events.clone();
        move |_| progress_events.set(progress_events.get() + 1)
    });
    fixture.batch.start_user_triggered();
    let request = take_request(&fixture.requests);
    finish(
        &fixture,
        &request,
        DownloadOutcome::Downloaded(fixture.root.path().join("cover.jpg")),
    );
    let generation = fixture.batch.generation_for_test();
    progress_events.set(0);

    project_return(&fixture.targets);

    assert_eq!(fixture.batch.generation_for_test(), generation);
    assert_eq!(progress_events.get(), 0);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn net_7c_online_to_online_starts_nothing_even_with_a_transient_failure_open() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let fixture = batch_fixture();
    fixture.batch.start_user_triggered();
    let request = take_request(&fixture.requests);
    finish(&fixture, &request, DownloadOutcome::TransientFailure);
    let generation = fixture.batch.generation_for_test();

    fixture.targets.project(Connectivity::Online);

    assert_eq!(fixture.batch.generation_for_test(), generation);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn net_7c_a_return_seen_during_a_failing_pass_retries_once_when_the_pass_ends() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let fixture = batch_fixture();
    fixture.batch.start_user_triggered();
    let first = take_request(&fixture.requests);
    let generation = fixture.batch.generation_for_test();

    project_return(&fixture.targets);
    assert_eq!(fixture.batch.generation_for_test(), generation);
    first
        .response
        .try_send(DownloadOutcome::TransientFailure)
        .unwrap();

    let retry = take_request(&fixture.requests);
    assert_eq!(fixture.batch.generation_for_test(), generation + 1);
    finish(
        &fixture,
        &retry,
        DownloadOutcome::Downloaded(fixture.root.path().join("cover.jpg")),
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn net_7c_cancel_and_a_new_pass_both_clear_a_pending_return() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let fixture = batch_fixture();
    fixture.batch.wait_for_network_return();
    fixture.batch.cancel();
    let after_cancel = fixture.batch.generation_for_test();
    project_return(&fixture.targets);
    assert_eq!(fixture.batch.generation_for_test(), after_cancel);

    fixture.batch.wait_for_network_return();
    fixture.batch.start_user_triggered();
    let request = take_request(&fixture.requests);
    finish(
        &fixture,
        &request,
        DownloadOutcome::Downloaded(fixture.root.path().join("cover.jpg")),
    );
    let after_pass = fixture.batch.generation_for_test();
    project_return(&fixture.targets);
    assert_eq!(fixture.batch.generation_for_test(), after_pass);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn net_7c_a_failed_pass_retries_once_on_the_next_return() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let fixture = batch_fixture();
    fixture.requests.close();
    fixture.batch.start_user_triggered();
    crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
        fixture.batch.progress_for_test().state == BatchState::Failed
            && !fixture.batch.running_for_test()
    });
    let generation = fixture.batch.generation_for_test();

    project_return(&fixture.targets);

    assert_eq!(fixture.batch.generation_for_test(), generation + 1);
    assert!(matches!(
        fixture.batch.progress_for_test(),
        BatchProgress {
            state: BatchState::Running | BatchState::Failed,
            ..
        }
    ));
}
