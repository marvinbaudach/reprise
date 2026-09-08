use std::path::PathBuf;

use super::machine::{DeviceSyncMachine, Effect, Event, SyncOutcome};
use super::mirror::LyricsSidecarWrite;
use super::{MirrorPlan, PlannedSyncPhase, SyncStep};

#[test]
fn a_lyrics_only_run_opens_on_and_executes_its_lyrics_write() {
    let mut plan = MirrorPlan::default();
    plan.lyrics_writes.push(LyricsSidecarWrite {
        track_id: 7,
        source_path: PathBuf::from("/music/song.lrc"),
        device_path: "Artist/Album/01 Song.lrc".into(),
        size_bytes: 42,
        existing_size_bytes: Some(40),
    });
    plan.transfer_bytes = 42;
    let mut machine = DeviceSyncMachine::new("serial-1".into(), plan);

    assert_eq!(
        machine.dispatch(Event::Start),
        vec![Effect::CleanPartials(Vec::new())]
    );
    assert_eq!(
        machine.phase(),
        &PlannedSyncPhase::Syncing {
            step: SyncStep::WritingLyrics,
            done: 0,
            total: 1,
            current_track: "Artist/Album/01 Song.lrc".into(),
            unit_bytes_done: 0,
            unit_bytes_total: 42,
        }
    );
    assert_eq!(
        machine.dispatch(Event::PartialsCleaned(Ok(()))),
        vec![Effect::WriteLyrics { index: 0 }]
    );
    assert_eq!(
        machine.dispatch(Event::LyricsWritten(Ok(42))),
        vec![Effect::Finished(SyncOutcome::Completed {
            verified_sources: Vec::new(),
        })]
    );
}
