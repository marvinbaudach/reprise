//! One seek on the pipeline is one seek at the parser (#1316).
//!
//! `playbin.seek_simple(FLUSH | ACCURATE, ..)` used to reach `flacparse` as
//! three flushing seeks with the same seqnum, a few milliseconds apart: the
//! audio-filter bin counts as a sink because it holds the CAVA appsink, so the
//! bin handed the event to it, to its own source pad and to the audio sink. The
//! back-to-back flushes let a sticky TAG go stale and fail the stream. The
//! filter bin now forwards the first SEEK of a seqnum and drops the rest; see
//! `crate::seek_dedupe`.
//!
//! The probe sits on the parser's source pad, upstream of every fan-in point,
//! on the production pipeline built through the [`Harness`]. The CUE case also
//! checks where playback starts, so a fix cannot trade the repeat for a wrong
//! start position.

use std::collections::{BTreeMap, HashSet};

use super::segment_support::{
    cue_item, encode_flac, on_every_pipeline, record_heard, write_regions_wav, Harness,
};
use super::*;

const TRACK_MS: u32 = 30_000;
const CUE_SEGMENT: (i64, i64) = (2_000, 20_000);
const USER_SEEK_MS: i64 = 5_000;
const PARSER_FACTORY: &str = "flacparse";
const HANG_GUARD: Duration = Duration::from_secs(10);
/// Time for a seek's copies and its first buffers to have passed the probe.
const SETTLE: Duration = Duration::from_millis(700);
/// How far past the requested start the first buffer heard may lie.
const LANDING_SLACK_MS: i64 = 250;
/// A whole-file seek is `KEY_UNIT` and may land on a key unit before its target;
/// the file sat near the start before it, so even this much slack proves the
/// seek moved the stream.
const KEY_UNIT_SLACK_MS: i64 = 1_500;

/// The SEEK events that reached the parser, by seqnum, and whether each flushed.
#[derive(Clone, Default)]
struct SeekLog {
    seeks: Arc<Mutex<Vec<(gst::Seqnum, bool)>>>,
    probed: Arc<Mutex<HashSet<usize>>>,
}

impl SeekLog {
    fn clear(&self) {
        self.seeks
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
    }

    /// How many SEEKs reached the parser per seqnum, and how many seqnums
    /// flushed.
    fn counts(&self) -> (BTreeMap<String, usize>, usize) {
        let seeks = self.seeks.lock().unwrap_or_else(PoisonError::into_inner);
        let mut per_seqnum = BTreeMap::new();
        for (seqnum, _) in seeks.iter() {
            *per_seqnum.entry(format!("{seqnum:?}")).or_default() += 1;
        }
        let flushing = seeks
            .iter()
            .filter(|(_, flush)| *flush)
            .map(|(seqnum, _)| format!("{seqnum:?}"))
            .collect::<HashSet<_>>()
            .len();
        (per_seqnum, flushing)
    }

    /// Whether this element has not been probed yet; marks it probed.
    fn first_visit(&self, element: &gst::Element) -> bool {
        self.probed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(element.as_ptr() as usize)
    }
}

fn is_parser(element: &gst::Element) -> bool {
    element
        .factory()
        .is_some_and(|factory| factory.name() == PARSER_FACTORY)
}

/// Probes the parser inside `element`, if there is one.
fn watch(log: &SeekLog, element: &gst::Element) {
    if let Some(bin) = element.downcast_ref::<gst::Bin>() {
        for child in bin.children() {
            watch(log, &child);
        }
        return;
    }
    if !is_parser(element) || !log.first_visit(element) {
        return;
    }
    let Some(src) = element.static_pad("src") else {
        return;
    };
    let log = log.clone();
    src.add_probe(gst::PadProbeType::EVENT_UPSTREAM, move |_, info| {
        if let Some(gst::PadProbeData::Event(event)) = &info.data {
            if let gst::EventView::Seek(seek) = event.view() {
                log.seeks
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push((event.seqnum(), seek.flags().contains(gst::SeekFlags::FLUSH)));
            }
        }
        gst::PadProbeReturn::Ok
    });
}

/// Probes the parser of this pipeline, whether it exists yet or is added while
/// the pipeline starts.
///
/// Runs once per pipeline the player builds, a rebuilt one included. The
/// visited set is keyed on element addresses, and glib may hand a freed
/// pipeline's address to the new parser, which would then go unprobed; so each
/// pipeline starts from an empty set.
fn watch_seeks(log: &SeekLog, playbin: &gst::Element) {
    let Some(bin) = playbin.downcast_ref::<gst::Bin>() else {
        return;
    };
    log.probed
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clear();
    let added = log.clone();
    bin.connect_deep_element_added(move |_, _, element| watch(&added, element));
    watch(log, playbin);
}

/// Asserts that exactly `flushing_seeks` distinct flushing seeks reached the
/// parser and that each seqnum reached it exactly once.
fn assert_one_seek_per_seqnum(case: &str, log: &SeekLog, flushing_seeks: usize) {
    let (per_seqnum, flushing) = log.counts();
    assert_eq!(
        flushing, flushing_seeks,
        "{case}: distinct flushing seeks at the parser (SEEKs per seqnum: {per_seqnum:?})"
    );
    assert!(
        per_seqnum.values().all(|&count| count == 1),
        "{case}: SEEKs that reached the parser per seqnum: {per_seqnum:?}"
    );
}

fn written_flac(directory: &tempfile::TempDir) -> std::path::PathBuf {
    let wav = directory.path().join("album.wav");
    let flac = directory.path().join("album.flac");
    write_regions_wav(&wav, &[(TRACK_MS, true)]);
    encode_flac(&wav, &flac);
    flac
}

fn playing(events: &[PlayerEvent]) -> bool {
    events
        .iter()
        .any(|event| matches!(event, PlayerEvent::StateChanged(PlaybackState::Playing)))
}

fn file_position_ms(player: &Player) -> i64 {
    let playbin = player
        .playbin
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    playbin
        .query_position::<gst::ClockTime>()
        .map_or(-1, |position| position.mseconds() as i64)
}

#[test]
fn a_cue_start_sends_one_seek_per_seqnum_to_the_parser() {
    let harness = Harness::new();
    let directory = tempfile::tempdir().unwrap();
    let flac = written_flac(&directory);
    let heard = record_heard(&harness.player);
    let log = SeekLog::default();
    on_every_pipeline(&harness.player, {
        let log = log.clone();
        move |playbin| watch_seeks(&log, playbin)
    });

    harness.start_resetting(
        || log.clear(),
        || {
            harness
                .player
                .play(cue_item(&flac, CUE_SEGMENT, 0.0))
                .unwrap();
        },
    );
    let events = harness.pump_until(HANG_GUARD, playing);
    assert!(playing(&events), "the CUE track must start playing");
    harness.pump_for(SETTLE);

    let landing = heard.latest_landing();
    let position_ms = file_position_ms(&harness.player);
    assert!(
        (CUE_SEGMENT.0..=CUE_SEGMENT.0 + LANDING_SLACK_MS).contains(&landing.start_ms),
        "the CUE track must play from its start ({} ms), not {} ms",
        CUE_SEGMENT.0,
        landing.start_ms
    );
    assert!(
        position_ms >= CUE_SEGMENT.0,
        "the CUE track must be at or past {} ms, not {position_ms} ms",
        CUE_SEGMENT.0
    );
    assert_one_seek_per_seqnum("CUE start at 2000 ms", &log, 1);
}

#[test]
fn a_user_seek_sends_one_seek_per_seqnum_to_the_parser() {
    let harness = Harness::new();
    let directory = tempfile::tempdir().unwrap();
    let flac = written_flac(&directory);
    let flac_path = flac.to_str().unwrap().to_owned();
    let log = SeekLog::default();
    on_every_pipeline(&harness.player, {
        let log = log.clone();
        move |playbin| watch_seeks(&log, playbin)
    });

    harness.start_resetting(
        || log.clear(),
        || harness.player.play(item(&flac_path)).unwrap(),
    );
    let events = harness.pump_until(HANG_GUARD, playing);
    assert!(playing(&events), "the whole file must start playing");
    harness.pump_for(SETTLE);

    log.clear();
    harness.player.seek_to(USER_SEEK_MS).unwrap();
    harness.pump_for(SETTLE);

    let position_ms = file_position_ms(&harness.player);
    assert!(
        position_ms >= USER_SEEK_MS - KEY_UNIT_SLACK_MS,
        "the seek must land within {KEY_UNIT_SLACK_MS} ms of {USER_SEEK_MS} ms, not at {position_ms} ms"
    );
    assert_one_seek_per_seqnum("user seek to 5000 ms", &log, 1);
}
