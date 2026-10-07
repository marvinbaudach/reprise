//! A track cut from a CUE file reaches Media3 as its own stretch of the
//! file, named by its own row (MTP-66), and the file's last track carries no
//! end, so it plays to the end of the file (MTP-68).

use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex};

use super::test_support::{Clip, PortCall, RecordingListener, RecordingPort};
use crate::cue_album_test_support::{cue_album, CueAlbum};
use crate::playback::PlaybackEventBridge;
use crate::{AndroidPlaybackSegment, AndroidPlaybackSession};

fn session_over(album: &CueAlbum) -> (AndroidPlaybackSession, Arc<Mutex<Vec<PortCall>>>) {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let session = AndroidPlaybackSession::new(
        Arc::clone(&album.library),
        Box::new(RecordingPort {
            calls: Arc::clone(&calls),
            bridge: Arc::new(Mutex::new(None::<Arc<PlaybackEventBridge>>)),
        }),
        Box::new(RecordingListener {
            snapshots: Arc::new(Mutex::new(Vec::new())),
            report_changes: Arc::new(AtomicUsize::new(0)),
        }),
    )
    .unwrap();
    calls.lock().unwrap().clear();
    (session, calls)
}

fn clip(album: &CueAlbum, index: usize, start_ms: i64, end_ms: Option<i64>) -> Clip {
    Clip {
        track_id: Some(album.track_ids[index]),
        uri: album.path.clone(),
        gain_db: 0.0,
        segment: AndroidPlaybackSegment { start_ms, end_ms },
    }
}

#[test]
fn mtp_66_a_cue_track_starts_as_its_own_stretch_and_the_next_one_is_fed_as_its_own() {
    let album = cue_album();
    let (session, calls) = session_over(&album);

    session.play_track_ids(album.track_ids.clone(), 0).unwrap();

    let calls = calls.lock().unwrap();
    assert!(
        calls.contains(&PortCall::PlayClip(clip(&album, 0, 0, Some(10_000)))),
        "{calls:?}"
    );
    assert!(
        calls.contains(&PortCall::SetNextClip(clip(
            &album,
            1,
            10_000,
            Some(20_000)
        ))),
        "{calls:?}"
    );
}

#[test]
fn mtp_68_the_last_track_of_a_cue_file_is_handed_over_without_an_end() {
    let album = cue_album();
    let (session, calls) = session_over(&album);

    session.play_track_ids(album.track_ids.clone(), 1).unwrap();
    assert!(calls
        .lock()
        .unwrap()
        .contains(&PortCall::SetNextClip(clip(&album, 2, 20_000, None))));

    calls.lock().unwrap().clear();
    session.next().unwrap();

    assert!(calls
        .lock()
        .unwrap()
        .contains(&PortCall::PlayClip(clip(&album, 2, 20_000, None))));
}
