use std::io;
use std::sync::{Arc, Mutex};

use reprise_core::agent_device_sync::{
    agent_device_sync_request, AgentDeviceSyncCommand, AgentDeviceSyncState,
};

use super::*;

#[derive(Clone, Default)]
struct CapturedLogs(Arc<Mutex<Vec<u8>>>);

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for CapturedLogs {
    type Writer = CapturedLogWriter;

    fn make_writer(&'a self) -> Self::Writer {
        CapturedLogWriter(self.0.clone())
    }
}

struct CapturedLogWriter(Arc<Mutex<Vec<u8>>>);

impl io::Write for CapturedLogWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn successful_agent_start_and_cancel_are_attributed_in_the_log() {
    let captured = CapturedLogs::default();
    let subscriber = tracing_subscriber::fmt()
        .without_time()
        .with_ansi(false)
        .with_max_level(tracing::Level::INFO)
        .with_writer(captured.clone())
        .finish();

    tracing::subscriber::with_default(subscriber, || {
        run(async {
            let (_temp, conn) = fixture();
            select_road_playlist(&conn, &[1]);
            let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 20));
            let runtime = DeviceSyncRuntime::with_backend(&conn, backend);
            settle_until("agent-startable device", || {
                runtime
                    .devices()
                    .first()
                    .is_some_and(|device| device.page.controls.can_start)
            })
            .await;

            let state = Arc::new(Mutex::new(AgentDeviceSyncState::default()));
            let (sender, receiver) = async_channel::unbounded();
            runtime.bind_agent_device_sync(&state, receiver);

            let (request, reply) = agent_device_sync_request(AgentDeviceSyncCommand::Start {
                device_name: "Phone a".into(),
            });
            sender.send(request).await.unwrap();
            gtk4::glib::timeout_future(Duration::from_millis(2)).await;
            assert_eq!(reply.try_recv(), Ok(Ok(())));

            let (request, reply) = agent_device_sync_request(AgentDeviceSyncCommand::Cancel {
                device_name: "Phone a".into(),
            });
            sender.send(request).await.unwrap();
            gtk4::glib::timeout_future(Duration::from_millis(2)).await;
            assert_eq!(reply.try_recv(), Ok(Ok(())));
        });
    });

    let log = String::from_utf8(captured.0.lock().unwrap().clone()).unwrap();
    let started = log
        .lines()
        .find(|line| line.contains("device sync started from agent"))
        .unwrap_or_else(|| panic!("missing start attribution: {log}"));
    assert!(started.contains("device_id=\"a\""), "{started}");
    let cancelled = log
        .lines()
        .find(|line| line.contains("device sync cancelled from agent"))
        .unwrap_or_else(|| panic!("missing cancel attribution: {log}"));
    assert!(cancelled.contains("device_id=\"a\""), "{cancelled}");
}
