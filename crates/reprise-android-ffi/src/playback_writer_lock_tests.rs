//! Transport calls arrive on Android's main thread. Two ANRs on the Pixel
//! (2026-09-03, 2026-09-14) show `next()` parked in `persist_queue` behind the
//! shared writer while a library scan walks the SAF tree holding it. A
//! transport call must therefore never wait for the writer.

use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use super::{
    AndroidPlaybackError, AndroidPlaybackPort, AndroidPlaybackState, AndroidTransitionMode,
    PlaybackEventBridge,
};
use crate::{
    AndroidEqualizerPoint, AndroidEqualizerSnapshot, AndroidPlaybackListener,
    AndroidPlaybackSession, AndroidPlaybackSnapshot,
};

/// How long a transport call may take to report completion while the writer is
/// still held. A call that merely enqueues its persistence finishes in
/// microseconds; this only bounds a regression that parks on the writer, so a
/// slow box cannot fail a correct call and a blocked one cannot hang the suite.
const TRANSPORT_COMPLETES_WITHIN: Duration = Duration::from_secs(10);

struct QuietPort;

impl AndroidPlaybackPort for QuietPort {
    fn set_event_bridge(
        &self,
        _bridge: Arc<PlaybackEventBridge>,
    ) -> Result<(), AndroidPlaybackError> {
        Ok(())
    }

    fn play_path(&self, _path: String, _gain_db: f64) -> Result<(), AndroidPlaybackError> {
        Ok(())
    }

    fn play_uri(&self, _uri: String) -> Result<(), AndroidPlaybackError> {
        Ok(())
    }

    fn toggle_pause(&self) -> Result<AndroidPlaybackState, AndroidPlaybackError> {
        Ok(AndroidPlaybackState::Paused)
    }

    fn seek_to(&self, _position_ms: i64) -> Result<(), AndroidPlaybackError> {
        Ok(())
    }

    fn set_volume(&self, _volume: f64) -> Result<(), AndroidPlaybackError> {
        Ok(())
    }

    fn set_equalizer(
        &self,
        _enabled: bool,
        _curve: Vec<AndroidEqualizerPoint>,
    ) -> Result<(), AndroidPlaybackError> {
        Ok(())
    }

    fn equalizer_snapshot(&self) -> Result<Option<AndroidEqualizerSnapshot>, AndroidPlaybackError> {
        Ok(None)
    }

    fn set_audio_effects(&self) -> Result<(), AndroidPlaybackError> {
        Ok(())
    }

    fn set_spectrum_enabled(&self, _enabled: bool) -> Result<(), AndroidPlaybackError> {
        Ok(())
    }

    fn stop(&self) -> Result<(), AndroidPlaybackError> {
        Ok(())
    }

    fn set_next(&self, _uri: Option<String>, _gain_db: f64) -> Result<(), AndroidPlaybackError> {
        Ok(())
    }

    fn set_transition(&self, _mode: AndroidTransitionMode) -> Result<(), AndroidPlaybackError> {
        Ok(())
    }

    fn current_generation(&self) -> Result<u64, AndroidPlaybackError> {
        Ok(0)
    }
}

struct QuietListener;

impl AndroidPlaybackListener for QuietListener {
    fn on_playback_changed(&self, _snapshot: AndroidPlaybackSnapshot) {}

    fn on_listen_report_changed(&self) {}
}

/// Runs `transport` on a helper thread while another thread holds the library
/// writer, the way a scan does, and returns whether it reported completion
/// *while the writer was still held*. The writer is released only afterwards,
/// by message rather than by a timer, so the verdict does not depend on how
/// fast the host is.
fn completes_while_writer_is_held(
    library: &Arc<crate::MusicLibrary>,
    transport: impl FnOnce() + Send,
) -> bool {
    let writer = library.writer_handle();
    let (held, wait_held) = mpsc::channel();
    let (release, wait_release) = mpsc::channel::<()>();
    let (done, wait_done) = mpsc::channel();
    thread::scope(|scope| {
        let holder = scope.spawn(move || {
            let guard = writer.lock().unwrap();
            held.send(()).unwrap();
            // A dropped sender releases too, so a failing test never pins the writer.
            let _ = wait_release.recv();
            drop(guard);
        });
        wait_held.recv().unwrap();
        scope.spawn(move || {
            transport();
            let _ = done.send(());
        });
        let completed = wait_done.recv_timeout(TRANSPORT_COMPLETES_WITHIN).is_ok();
        drop(release);
        holder.join().unwrap();
        completed
    })
}

fn session_with_three_tracks(
    directory: &std::path::Path,
) -> (Arc<crate::MusicLibrary>, AndroidPlaybackSession) {
    let library = super::test_support::library_in(directory);
    let session = AndroidPlaybackSession::new(
        Arc::clone(&library),
        Box::new(QuietPort),
        Box::new(QuietListener),
    )
    .unwrap();
    let ids = vec![1, 2, 3];
    let uris = ids
        .iter()
        .map(|id| format!("content://provider/{id}.flac"))
        .collect();
    session.play_tracks(ids, uris, 0).unwrap();
    (library, session)
}

#[test]
fn next_does_not_wait_for_a_scan_holding_the_writer() {
    let directory = tempfile::tempdir().unwrap();
    let (library, session) = session_with_three_tracks(directory.path());

    let completed = completes_while_writer_is_held(&library, || {
        session.next().unwrap();
    });

    assert!(
        completed,
        "next() did not finish while the writer was held -- on the main thread that is the ANR \
         'Input dispatching timed out' seen on the device",
    );
    assert_eq!(session.snapshot().unwrap().current_index, Some(1));
}

#[test]
fn play_tracks_does_not_wait_for_a_scan_holding_the_writer() {
    let directory = tempfile::tempdir().unwrap();
    let (library, session) = session_with_three_tracks(directory.path());

    let completed = completes_while_writer_is_held(&library, || {
        session
            .play_tracks(vec![7, 8], vec!["content://p/7".into(), "content://p/8".into()], 1)
            .unwrap();
    });

    assert!(
        completed,
        "play_tracks() did not finish while the writer was held",
    );
}
