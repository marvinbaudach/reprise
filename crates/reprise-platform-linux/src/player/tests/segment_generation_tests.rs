//! A CUE start is a stream start from the moment it is issued.
//!
//! Its pipeline only enters `Playing` once the file has prerolled, but the
//! frontend decides which faults belong to which start by the stream
//! generation. A start that bumped it only at preroll would leave a window in
//! which a stale fault of the previous stream carries the very generation the
//! CUE track is current under.

use super::segment_support::{cue_item, write_regions_wav, Harness};
use super::*;

const HANG_GUARD: Duration = Duration::from_secs(60);

#[test]
fn play_22_a_cue_start_bumps_the_stream_generation_when_it_is_issued() {
    let harness = Harness::new();
    let directory = tempfile::tempdir().unwrap();
    let album = directory.path().join("album.wav");
    write_regions_wav(&album, &[(1_000, false), (3_000, true)]);
    let before = harness.player.current_generation();
    let issued = std::cell::Cell::new(before);

    harness.start(|| {
        harness
            .player
            .play(cue_item(&album, (1_000, 2_500), 0.0))
            .unwrap();
        issued.set(harness.player.current_generation());
    });
    harness.pump_until(HANG_GUARD, |events| {
        events
            .iter()
            .any(|event| matches!(event, PlayerEvent::StateChanged(PlaybackState::Playing)))
    });

    assert!(
        issued.get() > before,
        "the generation must move when the start is issued, not at preroll"
    );
    assert_eq!(
        harness.player.current_generation(),
        issued.get(),
        "entering Playing is the same stream start, not another one"
    );
}
