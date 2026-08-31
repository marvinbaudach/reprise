use std::io;
use std::sync::{Arc, Mutex};

use super::*;

#[derive(Clone, Default)]
struct CapturedWarnings(Arc<Mutex<Vec<u8>>>);

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for CapturedWarnings {
    type Writer = CapturedWarningWriter;

    fn make_writer(&'a self) -> Self::Writer {
        CapturedWarningWriter(self.0.clone())
    }
}

struct CapturedWarningWriter(Arc<Mutex<Vec<u8>>>);

impl io::Write for CapturedWarningWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn missing_analysis_warns_with_the_track_id_during_planning() {
    let captured = CapturedWarnings::default();
    let subscriber = tracing_subscriber::fmt()
        .without_time()
        .with_ansi(false)
        .with_max_level(tracing::Level::WARN)
        .with_writer(captured.clone())
        .finish();

    tracing::subscriber::with_default(subscriber, || {
        run(async {
            let (_temp, conn) = fixture();
            select_road_playlist(&conn, &[1]);
            let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
            let _runtime = DeviceSyncRuntime::with_backend(&conn, backend);
            settle().await;
        });
    });

    let log = String::from_utf8(captured.0.lock().unwrap().clone()).unwrap();
    assert!(
        log.contains("analysis sidecar data is unavailable"),
        "{log}"
    );
    assert!(log.contains("track_id=1"), "{log}");
}
