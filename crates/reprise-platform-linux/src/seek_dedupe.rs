//! One seek reaches the decoder once (#1316).
//!
//! `playbin.seek_simple` sends the SEEK to every child of the bin that carries
//! the `SINK` flag, and a bin carries it as soon as one of its children is a
//! sink. The audio-filter bin holds the CAVA appsink, so playsink counts it as
//! a sink next to the real audio sink: the one seek goes up the CAVA branch,
//! out of the filter bin's own source pad, and up the audio sink's chain, and
//! all three copies come out of the filter bin's sink pad with the same
//! seqnum, a few milliseconds apart. The decoder chain then sees three
//! flushing seeks back to back; a sticky TAG goes stale and races the next
//! flush, GStreamer turns the FALSE sticky push into `GST_FLOW_ERROR`, and
//! `flacparse` fails the stream.
//!
//! The copies are one request, so the filter bin forwards the first and drops
//! the rest. A probe that drops an upstream event does not fail the push
//! (`gst_pad_push_event` counts `GST_FLOW_PROBE_DROPPED` as handled), so the
//! caller's `seek_simple` still returns `Ok`.

use gstreamer as gst;
use gstreamer::prelude::*;
use std::sync::{Mutex, PoisonError};

use reprise_core::playback::PlaybackError;

/// Whether the SEEK carrying `seqnum` goes on upstream, given the seqnum of the
/// last one that did. Records `seqnum` when it does.
pub(crate) fn forwards(last: &mut Option<gst::Seqnum>, seqnum: gst::Seqnum) -> bool {
    if *last == Some(seqnum) {
        return false;
    }
    *last = Some(seqnum);
    true
}

/// Makes `filter`, the audio-filter bin, pass each upstream SEEK on once.
///
/// Called by `build_audio_filter` on the bin it builds, so every pipeline that
/// gets the filter gets this probe: `apply_audio_filter`, called by
/// `build_playbin`, is the only place that installs a filter, and the initial
/// pipeline, the one a watchdog restart rebuilds and the crossfade secondary
/// all come from `build_playbin`. The probe sits on the bin's sink pad, which
/// all copies of a seek pass; the state is the last forwarded seqnum, and it
/// lives in the probe for as long as the bin does.
pub(crate) fn install_seek_dedupe(filter: &gst::Bin) -> Result<(), PlaybackError> {
    let sink = filter
        .static_pad("sink")
        .ok_or_else(|| PlaybackError::Backend("GStreamer: filter has no sink pad".into()))?;
    let last = Mutex::new(None::<gst::Seqnum>);
    sink.add_probe(gst::PadProbeType::EVENT_UPSTREAM, move |_, info| {
        let Some(gst::PadProbeData::Event(event)) = &info.data else {
            return gst::PadProbeReturn::Ok;
        };
        if event.type_() != gst::EventType::Seek {
            return gst::PadProbeReturn::Ok;
        }
        let mut last = last.lock().unwrap_or_else(PoisonError::into_inner);
        if forwards(&mut last, event.seqnum()) {
            gst::PadProbeReturn::Ok
        } else {
            gst::PadProbeReturn::Drop
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seqnums() -> (gst::Seqnum, gst::Seqnum) {
        gst::init().unwrap();
        (gst::Seqnum::next(), gst::Seqnum::next())
    }

    #[test]
    fn the_first_seek_of_a_seqnum_is_forwarded_and_its_copies_are_not() {
        let (first, _) = seqnums();
        let mut last = None;
        assert!(forwards(&mut last, first));
        assert!(!forwards(&mut last, first));
        assert!(!forwards(&mut last, first));
    }

    #[test]
    fn a_new_seqnum_is_forwarded_again() {
        let (first, second) = seqnums();
        let mut last = None;
        assert!(forwards(&mut last, first));
        assert!(forwards(&mut last, second));
        assert!(!forwards(&mut last, second));
        assert_eq!(last, Some(second));
    }
}
