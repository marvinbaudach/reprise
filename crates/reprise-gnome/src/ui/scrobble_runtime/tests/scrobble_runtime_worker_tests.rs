use super::*;

#[test]
fn unauthorized_worker_stops_without_deleting_offline_queue() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("worker.db");
    {
        let conn = reprise_core::db::open(Some(&path)).unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        let source = queued_conn();
        for listen in reprise_core::scrobbling::pending(&source, 100).unwrap() {
            reprise_core::scrobbling::enqueue(&conn, &listen).unwrap();
        }
    }
    let (_command_sender, command_receiver) = mpsc::channel();
    let (status_sender, status_receiver) = async_channel::unbounded();
    run_worker(
        WorkerConfig {
            database_path: &path,
            provider: ScrobbleProvider::ListenBrainz,
            service: "ListenBrainz",
            credential: "bad-token",
            generation: 9,
        },
        &command_receiver,
        &status_sender,
        &FakeTransport {
            validation: Err(TransportError::Unauthorized),
            result: Ok(()),
            submitted: Arc::new(Mutex::new(Vec::new())),
        },
        WorkerCoordination {
            drain_lock: &Mutex::new(()),
            cancelled: &AtomicBool::new(false),
        },
    );
    let mut statuses = Vec::new();
    while let Ok(status) = status_receiver.try_recv() {
        statuses.push(status);
    }
    assert!(statuses.contains(&(9, ConnectionStatus::Unauthorized)));
    let conn = reprise_core::db::open(Some(&path)).unwrap();
    reprise_core::db::migrate(&conn).unwrap();
    assert_eq!(reprise_core::scrobbling::pending_count(&conn).unwrap(), 2);
}

#[test]
fn cancelled_worker_performs_no_network_or_status_work() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("cancelled.db");
    let conn = reprise_core::db::open(Some(&path)).unwrap();
    reprise_core::db::migrate(&conn).unwrap();
    drop(conn);
    let (_command_sender, command_receiver) = mpsc::channel();
    let (status_sender, status_receiver) = async_channel::unbounded();
    let cancelled = AtomicBool::new(true);
    run_worker(
        WorkerConfig {
            database_path: &path,
            provider: ScrobbleProvider::ListenBrainz,
            service: "ListenBrainz",
            credential: "unused-token",
            generation: 10,
        },
        &command_receiver,
        &status_sender,
        &FakeTransport {
            validation: Err(TransportError::Unauthorized),
            result: Ok(()),
            submitted: Arc::new(Mutex::new(Vec::new())),
        },
        WorkerCoordination {
            drain_lock: &Mutex::new(()),
            cancelled: &cancelled,
        },
    );
    assert!(status_receiver.try_recv().is_err());
}

#[test]
fn lastfm_flush_acknowledges_only_the_lastfm_queue() {
    let conn = queued_conn();
    let listen = reprise_core::scrobbling::pending(&conn, 1)
        .unwrap()
        .pop()
        .unwrap();
    reprise_core::scrobbling::enqueue_for(&conn, ScrobbleProvider::LastFm, &listen).unwrap();
    let transport = FakeTransport {
        validation: Ok("tester".to_string()),
        result: Ok(()),
        submitted: Arc::new(Mutex::new(Vec::new())),
    };

    flush_pending(&conn, ScrobbleProvider::LastFm, &transport, "session-key").unwrap();

    assert_eq!(
        reprise_core::scrobbling::pending_count_for(&conn, ScrobbleProvider::LastFm).unwrap(),
        0
    );
    assert_eq!(reprise_core::scrobbling::pending_count(&conn).unwrap(), 2);
}
