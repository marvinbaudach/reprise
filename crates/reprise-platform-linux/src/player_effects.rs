use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;

use reprise_core::playback::{AudioEffects, PlaybackError};

pub(super) const CAVA_SINK_NAME: &str = "reprise-cava-sink";
pub(super) const CAVA_SAMPLE_RATE_HZ: i32 = 44_100;

pub(super) fn build_audio_filter(
    effects: &AudioEffects,
) -> Result<Option<gst::Element>, PlaybackError> {
    use reprise_core::library::settings::ReplayGainMode;
    let bin = gst::Bin::new();
    let first = gst::ElementFactory::make("audioconvert")
        .build()
        .map_err(|error| PlaybackError::Backend(format!("GStreamer: {error}")))?;
    let equalizer = gst::ElementFactory::make("equalizer-10bands")
        .name("reprise-equalizer")
        .build()
        .map_err(|error| PlaybackError::Backend(format!("GStreamer: {error}")))?;
    set_equalizer_bands(&equalizer, effects);
    let tee = gst::ElementFactory::make("tee")
        .name("reprise-analysis-tee")
        .build()
        .map_err(|error| PlaybackError::Backend(format!("GStreamer: {error}")))?;
    let playback_queue = gst::ElementFactory::make("queue")
        .name("reprise-playback-queue")
        .build()
        .map_err(|error| PlaybackError::Backend(format!("GStreamer: {error}")))?;
    let cava_queue = gst::ElementFactory::make("queue")
        .name("reprise-cava-queue")
        .build()
        .map_err(|error| PlaybackError::Backend(format!("GStreamer: {error}")))?;
    let cava_convert = gst::ElementFactory::make("audioconvert")
        .build()
        .map_err(|error| PlaybackError::Backend(format!("GStreamer: {error}")))?;
    let cava_resample = gst::ElementFactory::make("audioresample")
        .build()
        .map_err(|error| PlaybackError::Backend(format!("GStreamer: {error}")))?;
    let cava_caps = gst::Caps::builder("audio/x-raw")
        .field("format", "F32LE")
        .field("channels", 1_i32)
        .field("rate", CAVA_SAMPLE_RATE_HZ)
        .field("layout", "interleaved")
        .build();
    let cava_sink = gst_app::AppSink::builder()
        .caps(&cava_caps)
        .sync(true)
        .max_buffers(2)
        .drop(true)
        .enable_last_sample(false)
        .build();
    cava_sink.set_property("name", CAVA_SINK_NAME);

    let mut playback_elements = vec![playback_queue];
    if effects.replay_gain != ReplayGainMode::Off {
        let replaygain = gst::ElementFactory::make("rgvolume")
            .name("reprise-replaygain")
            .build()
            .map_err(|error| PlaybackError::Backend(format!("GStreamer: {error}")))?;
        replaygain.set_property("album-mode", effects.replay_gain == ReplayGainMode::Album);
        playback_elements.push(replaygain);
    }
    playback_elements.push(
        gst::ElementFactory::make("audioconvert")
            .build()
            .map_err(|error| PlaybackError::Backend(format!("GStreamer: {error}")))?,
    );
    let all_elements = [
        vec![first.clone(), equalizer.clone(), tee.clone()],
        playback_elements.clone(),
        vec![
            cava_queue.clone(),
            cava_convert.clone(),
            cava_resample.clone(),
            cava_sink.clone().upcast(),
        ],
    ]
    .concat();
    bin.add_many(all_elements.iter().collect::<Vec<_>>())
        .map_err(|error| PlaybackError::Backend(format!("GStreamer: {error}")))?;
    gst::Element::link_many([&first, &equalizer, &tee])
        .map_err(|error| PlaybackError::Backend(format!("GStreamer: {error}")))?;
    let playback_chain = std::iter::once(&tee)
        .chain(playback_elements.iter())
        .collect::<Vec<_>>();
    gst::Element::link_many(playback_chain)
        .map_err(|error| PlaybackError::Backend(format!("GStreamer: {error}")))?;
    gst::Element::link_many([
        &tee,
        &cava_queue,
        &cava_convert,
        &cava_resample,
        cava_sink.upcast_ref(),
    ])
    .map_err(|error| PlaybackError::Backend(format!("GStreamer: {error}")))?;
    let sink = first
        .static_pad("sink")
        .ok_or_else(|| PlaybackError::Backend("GStreamer: filter has no sink pad".into()))?;
    let src = playback_elements
        .last()
        .and_then(|element| element.static_pad("src"))
        .ok_or_else(|| PlaybackError::Backend("GStreamer: filter has no src pad".into()))?;
    bin.add_pad(
        &gst::GhostPad::with_target(&sink)
            .map_err(|error| PlaybackError::Backend(format!("GStreamer: {error}")))?,
    )
    .map_err(|error| PlaybackError::Backend(format!("GStreamer: {error}")))?;
    bin.add_pad(
        &gst::GhostPad::with_target(&src)
            .map_err(|error| PlaybackError::Backend(format!("GStreamer: {error}")))?,
    )
    .map_err(|error| PlaybackError::Backend(format!("GStreamer: {error}")))?;
    Ok(Some(bin.upcast()))
}

pub(super) fn set_spectrum_messages(
    filter: &gst::Element,
    enabled: bool,
) -> Result<(), PlaybackError> {
    let bin = filter
        .clone()
        .downcast::<gst::Bin>()
        .map_err(|_| PlaybackError::Backend("GStreamer: audio filter is not a bin".into()))?;
    let Some(_cava_sink) = bin.by_name(CAVA_SINK_NAME) else {
        return if enabled {
            Err(PlaybackError::Backend(
                "GStreamer: audio filter has no CAVA PCM sink".into(),
            ))
        } else {
            Ok(())
        };
    };
    Ok(())
}

pub(super) fn set_playbin_spectrum_messages(
    playbin: &gst::Element,
    enabled: bool,
) -> Result<(), PlaybackError> {
    let filter = playbin
        .property::<Option<gst::Element>>("audio-filter")
        .ok_or_else(|| PlaybackError::Backend("GStreamer: playbin has no audio filter".into()))?;
    set_spectrum_messages(&filter, enabled)
}

fn set_equalizer_bands(equalizer: &gst::Element, effects: &AudioEffects) {
    for (index, value) in effects.equalizer_bands.iter().enumerate() {
        let gain = if effects.equalizer_enabled {
            value.clamp(-12.0, 12.0)
        } else {
            0.0
        };
        equalizer.set_property(&format!("band{index}"), gain);
    }
}

pub(super) fn apply_audio_filter(
    playbin: &gst::Element,
    effects: &AudioEffects,
) -> Result<(), PlaybackError> {
    let filter = build_audio_filter(effects)?;
    playbin.set_property("audio-filter", filter.as_ref());
    Ok(())
}

pub(super) fn same_filter_topology(current: &AudioEffects, next: &AudioEffects) -> bool {
    use reprise_core::library::settings::ReplayGainMode;
    (current.replay_gain != ReplayGainMode::Off) == (next.replay_gain != ReplayGainMode::Off)
}

/// Updates properties on the existing filter bin when no elements need to be
/// added or removed. The equalizer is always present with neutral bands while
/// disabled, so enabling it never requires a pipeline state transition.
pub(super) fn update_existing_audio_filter(
    playbin: &gst::Element,
    current: &AudioEffects,
    next: &AudioEffects,
) -> bool {
    use reprise_core::library::settings::ReplayGainMode;
    if !same_filter_topology(current, next) {
        return false;
    }
    let Some(filter) = playbin.property::<Option<gst::Element>>("audio-filter") else {
        return false;
    };
    let Ok(bin) = filter.downcast::<gst::Bin>() else {
        return false;
    };
    let Some(equalizer) = bin.by_name("reprise-equalizer") else {
        return false;
    };
    set_equalizer_bands(&equalizer, next);
    if next.replay_gain != ReplayGainMode::Off {
        let Some(replaygain) = bin.by_name("reprise-replaygain") else {
            return false;
        };
        replaygain.set_property("album-mode", next.replay_gain == ReplayGainMode::Album);
    }
    true
}

pub(super) fn requested_state(element: &gst::Element) -> gst::State {
    let (_, current, pending) = element.state(gst::ClockTime::ZERO);
    if pending == gst::State::VoidPending {
        current
    } else {
        pending
    }
}

fn restore_requested_state(
    playbin: &gst::Element,
    state: gst::State,
    position: Option<gst::ClockTime>,
) -> Result<(), PlaybackError> {
    if state == gst::State::Null {
        return Ok(());
    }
    playbin
        .set_state(state)
        .map_err(|error| PlaybackError::Backend(format!("GStreamer: {error}")))?;
    if let Some(position) = position {
        let _ = playbin.seek_simple(gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT, position);
    }
    Ok(())
}

pub(super) fn replace_audio_filter(
    playbin: &gst::Element,
    effects: &AudioEffects,
    apply: impl FnOnce(&gst::Element, &AudioEffects) -> Result<(), PlaybackError>,
) -> Result<(), PlaybackError> {
    let state = requested_state(playbin);
    let position = playbin.query_position::<gst::ClockTime>();
    playbin
        .set_state(gst::State::Null)
        .map_err(|error| PlaybackError::Backend(format!("GStreamer: {error}")))?;
    let apply_result = apply(playbin, effects);
    let restore_result = restore_requested_state(playbin, state, position);
    match (apply_result, restore_result) {
        (Err(error), Err(restore_error)) => {
            tracing::warn!(%restore_error, "could not restore playback after filter failure");
            Err(error)
        }
        (Err(error), Ok(())) => Err(error),
        (Ok(()), Err(error)) => Err(error),
        (Ok(()), Ok(())) => Ok(()),
    }
}
