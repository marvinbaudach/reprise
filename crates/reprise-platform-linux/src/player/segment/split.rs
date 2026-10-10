//! Switching to the next track's gain at the exact sample of a contiguous
//! boundary (PLAY-23a).
//!
//! The boundary probe sees whole buffers, and a boundary seldom falls on a
//! buffer's edge: the buffer that holds it starts in the outgoing track and
//! runs on into the next one. Handed over whole it would play at the outgoing
//! gain to its end, up to one buffer — tens of milliseconds, at a gain that can
//! be 10 dB off — into the next track. So that one buffer is cut in two at the
//! boundary frame instead: the first part is pushed at the outgoing gain, the
//! gain switches, and the second part is pushed at the next track's.
//!
//! The parts go into the gain element's sink pad from inside its own probe,
//! which is safe because a pad's stream lock is recursive. While they do, the
//! probe lets them through untouched: the decision about this boundary has
//! been taken once, for the buffer as a whole.

use std::sync::atomic::{AtomicBool, Ordering};

use gstreamer as gst;
use gstreamer::prelude::*;

use super::SegmentGate;
use crate::player_effects::linear_gain;

const NANOS_PER_SECOND: u64 = 1_000_000_000;
/// `GST_BUFFER_OFFSET_NONE`: the buffer carries no frame offset.
const OFFSET_NONE: u64 = u64::MAX;

/// What the probe needs of a buffer's layout to cut it at a frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct FrameLayout {
    rate: u64,
    bytes_per_frame: usize,
}

impl FrameLayout {
    /// The layout of interleaved raw audio, or `None` for anything else — a
    /// buffer of a format this cannot cut keeps passing whole.
    pub(super) fn from_caps(caps: &gst::CapsRef) -> Option<Self> {
        let structure = caps.structure(0)?;
        if structure.name() != "audio/x-raw" {
            return None;
        }
        let rate = u64::try_from(structure.get::<i32>("rate").ok()?).ok()?;
        let channels = usize::try_from(structure.get::<i32>("channels").ok()?).ok()?;
        let bytes_per_sample = bytes_per_sample(structure.get::<&str>("format").ok()?)?;
        (rate > 0 && channels > 0).then_some(Self {
            rate,
            bytes_per_frame: bytes_per_sample * channels,
        })
    }

    fn frames(&self, bytes: usize) -> u64 {
        (bytes / self.bytes_per_frame) as u64
    }

    fn duration(&self, frames: u64) -> gst::ClockTime {
        gst::ClockTime::from_nseconds(NANOS_PER_SECOND)
            .mul_div_floor(frames, self.rate)
            .unwrap_or(gst::ClockTime::ZERO)
    }
}

/// The width of one sample of a raw audio format such as `S16LE`, `F32LE` or
/// `S24_32LE`.
fn bytes_per_sample(format: &str) -> Option<usize> {
    let bits = format.get(1..)?;
    let bits = bits.strip_suffix("LE").or(bits.strip_suffix("BE"))?;
    if !matches!(format.as_bytes().first()?, b'S' | b'U' | b'F') {
        return None;
    }
    // `S24_32LE` is 24 bits in a 32-bit container.
    let container = bits.rsplit('_').next()?;
    Some(
        container
            .parse::<usize>()
            .ok()
            .filter(|bits| bits % 8 == 0)?
            / 8,
    )
}

/// The boundary a buffer holds, staged.
struct StagedSplit {
    head_frames: u64,
    epoch: u64,
}

impl SegmentGate {
    /// Stages the contiguous hand-off if the buffer starting at `stream_time`
    /// holds the boundary strictly inside it, and returns the frame it falls on
    /// and the hand-off's epoch. The gain is left as it is: the part before the
    /// boundary still plays at the outgoing one.
    fn stage_split(
        &self,
        stream_time: gst::ClockTime,
        frames: u64,
        layout: FrameLayout,
        gain: &gst::Element,
    ) -> Option<StagedSplit> {
        let mut state = self.lock();
        if state.pending.is_some() || state.armed.is_none() {
            return None;
        }
        let boundary_ns = state.active?.boundary_ns()?;
        let into_buffer_ns = boundary_ns.checked_sub(stream_time.nseconds())?;
        let head_frames = (u128::from(into_buffer_ns) * u128::from(layout.rate)
            + u128::from(NANOS_PER_SECOND) / 2)
            / u128::from(NANOS_PER_SECOND);
        let head_frames = u64::try_from(head_frames).ok()?;
        if head_frames == 0 || head_frames >= frames {
            return None;
        }
        let next = state.armed.take()?;
        let outgoing_gain = gain.property::<f64>("volume");
        tracing::debug!(
            boundary_ms = next.cut.start_ms,
            head_frames,
            "cue: contiguous hand-off staged inside a buffer"
        );
        let epoch = self.stage_handoff(&mut state, next, outgoing_gain);
        Some(StagedSplit { head_frames, epoch })
    }

    /// Switches the gain to the staged successor's, unless a seek withdrew the
    /// hand-off in the meantime. Returns whether it did.
    fn switch_to_staged_gain(&self, epoch: u64, gain: &gst::Element) -> bool {
        let state = self.lock();
        let Some(pending) = state.pending.filter(|pending| pending.epoch == epoch) else {
            return false;
        };
        gain.set_property("volume", linear_gain(pending.next.gain_db));
        true
    }
}

/// Cuts `buffer` into the parts before and after `head_frames`.
fn cut(
    buffer: &gst::BufferRef,
    layout: FrameLayout,
    head_frames: u64,
) -> Option<(gst::Buffer, gst::Buffer)> {
    let head_bytes = head_frames as usize * layout.bytes_per_frame;
    let tail_bytes = buffer.size().checked_sub(head_bytes)?;
    let total_frames = layout.frames(buffer.size());
    let copy = gst::BufferCopyFlags::FLAGS
        | gst::BufferCopyFlags::TIMESTAMPS
        | gst::BufferCopyFlags::META
        | gst::BufferCopyFlags::MEMORY;
    let mut head = buffer.copy_region(copy, 0..head_bytes).ok()?;
    let mut tail = buffer
        .copy_region(copy, head_bytes..head_bytes + tail_bytes)
        .ok()?;
    let head_duration = layout.duration(head_frames);
    {
        let head = head.get_mut()?;
        head.set_pts(buffer.pts());
        head.set_dts(buffer.dts());
        head.set_duration(head_duration);
        if buffer.offset() != OFFSET_NONE {
            head.set_offset_end(buffer.offset() + head_frames);
        }
    }
    {
        let tail = tail.get_mut()?;
        tail.set_pts(buffer.pts().map(|pts| pts + head_duration));
        tail.set_dts(buffer.dts().map(|dts| dts + head_duration));
        tail.set_duration(layout.duration(total_frames - head_frames));
        tail.unset_flags(gst::BufferFlags::DISCONT);
        if buffer.offset() != OFFSET_NONE {
            tail.set_offset(buffer.offset() + head_frames);
            tail.set_offset_end(buffer.offset_end());
        }
    }
    Some((head, tail))
}

/// Puts the parts back in the pad's chain without the probe deciding about them
/// again, and takes the flag down when it goes out of scope.
pub(super) struct PartsInFlight(AtomicBool);

impl PartsInFlight {
    pub(super) fn new() -> Self {
        Self(AtomicBool::new(false))
    }

    /// Whether the buffer at the probe is one of the parts being pushed.
    pub(super) fn is_a_part(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// What became of a buffer that holds the boundary.
pub(super) enum Split {
    /// Pushed as two parts; the buffer itself must not go on.
    Parts { epoch: u64 },
    /// Could not be cut: the hand-off is staged with the next gain already
    /// set, and the buffer passes whole, as it did before it was split.
    Whole { epoch: u64 },
}

/// If `buffer` holds the armed contiguous boundary inside it, stages the
/// hand-off and pushes the buffer into `pad` as two parts with the gain
/// switching between them. `None` leaves the buffer to pass untouched.
pub(super) fn split_at_boundary(
    gate: &SegmentGate,
    gain: &gst::Element,
    pad: &gst::Pad,
    buffer: &gst::BufferRef,
    stream_time: gst::ClockTime,
    parts: &PartsInFlight,
) -> Option<Split> {
    let layout = FrameLayout::from_caps(pad.current_caps()?.as_ref())?;
    let frames = layout.frames(buffer.size());
    let staged = gate.stage_split(stream_time, frames, layout, gain)?;
    let Some((head, tail)) = cut(buffer, layout, staged.head_frames) else {
        // Cannot happen for a buffer the boundary lies inside.
        gate.switch_to_staged_gain(staged.epoch, gain);
        return Some(Split::Whole {
            epoch: staged.epoch,
        });
    };
    parts.0.store(true, Ordering::SeqCst);
    let head_pushed = pad.chain(head).is_ok();
    // The staged track's gain stands whether or not the head went through.
    if gate.switch_to_staged_gain(staged.epoch, gain) && head_pushed {
        let _ = pad.chain(tail);
    }
    parts.0.store(false, Ordering::SeqCst);
    Some(Split::Parts {
        epoch: staged.epoch,
    })
}

#[cfg(test)]
mod tests {
    use super::bytes_per_sample;

    #[test]
    fn the_width_of_a_raw_audio_sample_follows_its_format_name() {
        assert_eq!(bytes_per_sample("S16LE"), Some(2));
        assert_eq!(bytes_per_sample("F32LE"), Some(4));
        assert_eq!(bytes_per_sample("F64BE"), Some(8));
        assert_eq!(bytes_per_sample("S24LE"), Some(3));
        assert_eq!(bytes_per_sample("S24_32LE"), Some(4));
        assert_eq!(bytes_per_sample("S8"), None);
        assert_eq!(bytes_per_sample("ENCODED"), None);
        assert_eq!(bytes_per_sample(""), None);
    }
}
