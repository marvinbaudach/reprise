use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use std::sync::PoisonError;

use reprise_core::library::loudness::{MAX_GAIN_DB, MIN_GAIN_DB};
use reprise_core::playback::{AudioEffects, PlaybackError};

pub(super) const CAVA_SINK_NAME: &str = "reprise-cava-sink";
pub(super) const TRACK_GAIN_NAME: &str = "reprise-track-gain";
pub(super) const CAVA_SAMPLE_RATE_HZ: i32 = 44_100;
// Keep about 130 ms of 60 Hz analysis buffers for short scheduling stalls, but
// stay bounded because this branch must never back-pressure audible playback.
const CAVA_SINK_MAX_QUEUED_BUFFERS: u32 = 8;

pub(super) fn build_audio_filter(
    effects: &AudioEffects,
) -> Result<Option<gst::Element>, PlaybackError> {
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
    let cava_capsfilter = gst::ElementFactory::make("capsfilter")
        .property("caps", &cava_caps)
        .build()
        .map_err(|error| PlaybackError::Backend(format!("GStreamer: {error}")))?;
    let cava_splitter = gst::ElementFactory::make("audiobuffersplit")
        .property("output-buffer-duration", gst::Fraction::new(1, 60))
        .build()
        .map_err(|error| {
            PlaybackError::Backend(format!(
                "GStreamer: audiobuffersplit requires gst-plugins-bad: {error}"
            ))
        })?;
    let cava_sink = gst_app::AppSink::builder()
        .caps(&cava_caps)
        .sync(true)
        .max_buffers(CAVA_SINK_MAX_QUEUED_BUFFERS)
        .drop(true)
        .enable_last_sample(false)
        .build();
    cava_sink.set_property("name", CAVA_SINK_NAME);

    let track_gain = gst::ElementFactory::make("volume")
        .name(TRACK_GAIN_NAME)
        .build()
        .map_err(|error| PlaybackError::Backend(format!("GStreamer: {error}")))?;
    let mut playback_elements = vec![playback_queue, track_gain];
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
            cava_capsfilter.clone(),
            cava_splitter.clone(),
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
        &cava_capsfilter,
        &cava_splitter,
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

/// Applies `effects` to the filter bin that is already installed. The filter
/// has a fixed topology: the equalizer is always present with neutral bands
/// while disabled, and the track gain is always present, so a live change never
/// needs a pipeline state transition. A bin that lacks either element is a
/// construction bug, reported rather than papered over by a rebuild.
pub(super) fn update_existing_audio_filter(
    playbin: &gst::Element,
    next: &AudioEffects,
) -> Result<(), PlaybackError> {
    let bin = playbin
        .property::<Option<gst::Element>>("audio-filter")
        .ok_or_else(|| PlaybackError::Backend("GStreamer: playbin has no audio filter".into()))?
        .downcast::<gst::Bin>()
        .map_err(|_| PlaybackError::Backend("GStreamer: audio filter is not a bin".into()))?;
    let equalizer = bin
        .by_name("reprise-equalizer")
        .ok_or_else(|| PlaybackError::Backend("GStreamer: audio filter has no equalizer".into()))?;
    if bin.by_name(TRACK_GAIN_NAME).is_none() {
        return Err(PlaybackError::Backend(
            "GStreamer: audio filter has no track gain".into(),
        ));
    }
    set_equalizer_bands(&equalizer, next);
    Ok(())
}

pub(super) fn set_playbin_track_gain(
    playbin: &gst::Element,
    gain_db: f64,
) -> Result<(), PlaybackError> {
    let filter = playbin
        .property::<Option<gst::Element>>("audio-filter")
        .ok_or_else(|| PlaybackError::Backend("GStreamer: playbin has no audio filter".into()))?;
    let bin = filter
        .downcast::<gst::Bin>()
        .map_err(|_| PlaybackError::Backend("GStreamer: audio filter is not a bin".into()))?;
    let gain = bin.by_name(TRACK_GAIN_NAME).ok_or_else(|| {
        PlaybackError::Backend("GStreamer: audio filter has no track gain".into())
    })?;
    gain.set_property("volume", linear_gain(gain_db));
    Ok(())
}

/// The `volume` factor for a gain in decibels. A non-finite gain plays at unity
/// and the gain is clamped to the window Core resolves into (-24..+12 dB, well
/// inside the element's `0..=10` range), so a bad value from any caller can
/// neither mute the stream by accident nor blast it.
pub(super) fn linear_gain(gain_db: f64) -> f64 {
    if !gain_db.is_finite() {
        return 1.0;
    }
    10_f64.powf(gain_db.clamp(MIN_GAIN_DB, MAX_GAIN_DB) / 20.0)
}

pub(super) fn install_stream_start_gain_switch(
    playbin: &gst::Element,
    pending_gain: crate::gapless::PendingGain,
) -> Result<(), PlaybackError> {
    let filter = playbin
        .property::<Option<gst::Element>>("audio-filter")
        .ok_or_else(|| PlaybackError::Backend("GStreamer: playbin has no audio filter".into()))?;
    install_filter_gain_switch(&filter, pending_gain)
}

/// Applies the pending gain when the next stream's `STREAM_START` reaches the
/// gain element, in the streaming thread, before that stream's first buffer.
/// The probe sits on the gain element's own sink pad, behind the playback
/// queue: the queue holds up to a second of the old track's tail, and only
/// there is the event serialised with that data. A probe on the filter bin's
/// sink pad would switch the gain before the tail is played.
pub(super) fn install_filter_gain_switch(
    filter: &gst::Element,
    pending_gain: crate::gapless::PendingGain,
) -> Result<(), PlaybackError> {
    let bin = filter
        .clone()
        .downcast::<gst::Bin>()
        .map_err(|_| PlaybackError::Backend("GStreamer: audio filter is not a bin".into()))?;
    let gain = bin.by_name(TRACK_GAIN_NAME).ok_or_else(|| {
        PlaybackError::Backend("GStreamer: audio filter has no track gain".into())
    })?;
    let sink = gain
        .static_pad("sink")
        .ok_or_else(|| PlaybackError::Backend("GStreamer: track gain has no sink pad".into()))?;
    let element = gain.clone();
    sink.add_probe(gst::PadProbeType::EVENT_DOWNSTREAM, move |_, info| {
        if info
            .event()
            .is_some_and(|event| event.type_() == gst::EventType::StreamStart)
        {
            let next = pending_gain
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take();
            if let Some(gain_db) = next {
                element.set_property("volume", linear_gain(gain_db));
            }
        }
        gst::PadProbeReturn::Ok
    });
    Ok(())
}
