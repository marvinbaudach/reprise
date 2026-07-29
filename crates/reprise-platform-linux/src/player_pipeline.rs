//! Building and wiring the GStreamer pipeline `Player` drives.
//!
//! Split out of `player.rs` when that file reached the repository's 800-line
//! ceiling, and along a seam that already existed: everything here is a free
//! function that `Player`, the crossfade ramp and the gapless hand-off all
//! call, none of it reaches into `Player`'s own state. Keeping it beside the
//! type rather than inside it costs nothing and stops the two growing into
//! each other.

use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use reprise_core::playback::{
    AudioEffects, BassPressureDetector, CavaBarProcessor, CavaConfig, PlaybackError, PlayerEvent,
    SpectrumFrame, SPECTRUM_BAND_COUNT,
};

use crate::crossfade::Transition;
use crate::gapless::{connect_about_to_finish, note_stream_start, HandoffFlag, NextUri};
use crate::player_effects::{apply_audio_filter, CAVA_SAMPLE_RATE_HZ, CAVA_SINK_NAME};

pub fn path_to_uri(path: &str) -> Result<String, PlaybackError> {
    if !path.starts_with('/') {
        return Err(PlaybackError::BadPath(path.into()));
    }
    gst::glib::filename_to_uri(path, None)
        .map(|u| u.to_string())
        .map_err(|e| PlaybackError::BadPath(e.to_string()))
}

pub fn validated_playback_uri(uri: &str) -> Result<String, PlaybackError> {
    let accepted = ["http://", "https://", "file://"]
        .iter()
        .any(|prefix| uri.starts_with(prefix) && uri.len() > prefix.len());
    if accepted {
        Ok(uri.to_owned())
    } else {
        Err(PlaybackError::BadPath(uri.into()))
    }
}

pub(crate) fn merge_stream_tags(
    previous: &(Option<String>, Option<String>),
    title: Option<String>,
    organization: Option<String>,
) -> Option<(Option<String>, Option<String>)> {
    let next = (
        title.or_else(|| previous.0.clone()),
        organization.or_else(|| previous.1.clone()),
    );
    (next != *previous).then_some(next)
}

/// Environment variable that, when set, overrides playbin's audio sink
/// element (e.g. `fakesink`). Used for headless verification in environments
/// without a real audio device.
pub(crate) const AUDIO_SINK_ENV_VAR: &str = "REPRISE_AUDIO_SINK";

/// Builds a fresh `playbin3` element with the `REPRISE_AUDIO_SINK` override
/// applied, if set. Extracted out of `Player::new` so `Player::rebuild_
/// playbin` (the wedged-pipeline recovery — see `Player::play`'s doc comment)
/// and the crossfade ramp (which builds the identically-configured *secondary*
/// pipeline — see `crossfade.rs`) can build matching elements. `pub(crate)` for
/// that second caller.
pub(crate) fn build_playbin(
    effects: &AudioEffects,
    next_uri: NextUri,
    handoff_pending: HandoffFlag,
    transition: Transition,
    stream_generation: Arc<AtomicU64>,
) -> Result<gst::Element, PlaybackError> {
    let playbin = gst::ElementFactory::make("playbin3")
        .build()
        .map_err(|e| PlaybackError::Backend(format!("GStreamer: {e}")))?;
    apply_audio_filter(&playbin, effects)?;

    // Gapless handoff: consume any pre-fed URI on `about-to-finish` without a
    // pipeline restart (Gapless mode only — the handler no-ops in Crossfade/Off,
    // see `gapless.rs`). Installed here so `rebuild_playbin` and the crossfade
    // secondary re-arm it on the built element for free.
    connect_about_to_finish(
        &playbin,
        next_uri,
        handoff_pending,
        transition,
        stream_generation,
    );

    if let Ok(sink_name) = std::env::var(AUDIO_SINK_ENV_VAR) {
        let sink = gst::ElementFactory::make(&sink_name)
            .build()
            .map_err(|e| PlaybackError::Backend(format!("GStreamer: {e}")))?;
        // Pace the override sink against the pipeline clock (if it has a
        // `sync` property): `fakesink` defaults to sync=false, which
        // would consume an entire track as fast as it decodes — EOS
        // after milliseconds, no position ticks — making headless runs
        // behave nothing like real playback. Real audio sinks default to
        // sync=true anyway, so this only affects test sinks.
        //
        // `find_property` only confirms a property named "sync" exists,
        // not that it's a `bool` — `set_property` panics on a type
        // mismatch. This path only runs for developer-chosen
        // `REPRISE_AUDIO_SINK` overrides (never in production), but an
        // exotic element with an unrelated "sync" property (wrong type)
        // must not be able to crash a headless dev run, so check the
        // property's declared type before setting it.
        let has_bool_sync = sink
            .find_property("sync")
            .is_some_and(|pspec| pspec.value_type() == gst::glib::Type::BOOL);
        if has_bool_sync {
            sink.set_property("sync", true);
        }
        tracing::info!(sink = %sink_name, "REPRISE_AUDIO_SINK override active");
        playbin.set_property("audio-sink", &sink);
    }

    Ok(playbin)
}

pub(crate) fn attach_cava_sink(
    playbin: &gst::Element,
    on_event: Arc<dyn Fn(PlayerEvent) + Send + Sync>,
    enabled: Arc<AtomicBool>,
    stream_generation: Arc<AtomicU64>,
) -> Result<(), PlaybackError> {
    let filter = playbin
        .property::<Option<gst::Element>>("audio-filter")
        .ok_or_else(|| PlaybackError::Backend("GStreamer: playbin has no audio filter".into()))?;
    let bin = filter
        .downcast::<gst::Bin>()
        .map_err(|_| PlaybackError::Backend("GStreamer: audio filter is not a bin".into()))?;
    let sink = bin
        .by_name(CAVA_SINK_NAME)
        .ok_or_else(|| PlaybackError::Backend("GStreamer: filter has no CAVA PCM sink".into()))?
        .downcast::<gst_app::AppSink>()
        .map_err(|_| PlaybackError::Backend("GStreamer: CAVA sink is not an AppSink".into()))?;
    let config = CavaConfig::new(CAVA_SAMPLE_RATE_HZ as u32, SPECTRUM_BAND_COUNT);
    let mut processor = CavaBarProcessor::new(config)
        .map_err(|error| PlaybackError::Backend(format!("CAVA: {error}")))?;
    // Measured from the same PCM, but deliberately outside CAVA: the bars are
    // auto-sensitivity-normalized and cannot say how loud the bass really is.
    let mut pressure_detector = BassPressureDetector::new(CAVA_SAMPLE_RATE_HZ as u32);
    let mut was_enabled = false;
    let mut seen_stream_generation = stream_generation.load(Ordering::Acquire);
    sink.set_callbacks(
        gst_app::AppSinkCallbacks::builder()
            .new_sample(move |sink| {
                let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                if !enabled.load(Ordering::Relaxed) {
                    was_enabled = false;
                    return Ok(gst::FlowSuccess::Ok);
                }
                let current_stream_generation = stream_generation.load(Ordering::Acquire);
                if !was_enabled || current_stream_generation != seen_stream_generation {
                    processor.reset();
                    pressure_detector.reset();
                    was_enabled = true;
                    seen_stream_generation = current_stream_generation;
                }
                let buffer = sample.buffer().ok_or(gst::FlowError::Error)?;
                if buffer.flags().contains(gst::BufferFlags::DISCONT) {
                    processor.reset();
                    pressure_detector.reset();
                }
                let map = buffer.map_readable().map_err(|_| gst::FlowError::Error)?;
                let pcm = map
                    .as_slice()
                    .chunks_exact(size_of::<f32>())
                    .map(|bytes| f32::from_le_bytes(bytes.try_into().expect("four-byte PCM chunk")))
                    .collect::<Vec<_>>();
                let bands: [f32; SPECTRUM_BAND_COUNT] = processor
                    .process(&pcm)
                    .try_into()
                    .expect("the CAVA processor returns its configured bar count");
                let pressure = pressure_detector.observe(&pcm);
                (*on_event)(PlayerEvent::Spectrum(
                    SpectrumFrame::from_cava_bars(bands).with_bass_pressure(pressure),
                ));
                Ok(gst::FlowSuccess::Ok)
            })
            .build(),
    );
    Ok(())
}

/// Attaches a bus watch to `playbin` that reports EOS/error messages via
/// `on_event`. Extracted out of `Player::new` so `Player::rebuild_playbin` and
/// the crossfade promotion (see `crossfade.rs`) can attach an identically-
/// behaving watch to a replacement/promoted element (a `BusWatchGuard`/`Bus` is
/// tied to the specific element it came from, so a rebuilt/promoted playbin
/// needs its own watch rather than reusing the old one). `pub(crate)` for the
/// crossfade caller.
///
/// `crossfading` gates the EOS→`TrackFinished` emission: while a crossfade is in
/// flight the *outgoing* pipeline (which still holds this watch until promotion)
/// naturally ends if the track is shorter than the fade overlap; that EOS must
/// not surface as a spurious `TrackFinished`, because the crossfade promotion is
/// the authoritative advance and emits `AdvancedToNext` itself.
pub(crate) fn attach_bus_watch(
    playbin: &gst::Element,
    on_event: Arc<dyn Fn(PlayerEvent) + Send + Sync>,
    handoff_pending: HandoffFlag,
    crossfading: Arc<AtomicBool>,
    spectrum_enabled: Arc<AtomicBool>,
    cava_stream_generation: Arc<AtomicU64>,
) -> Result<gst::bus::BusWatchGuard, PlaybackError> {
    attach_cava_sink(
        playbin,
        on_event.clone(),
        spectrum_enabled,
        cava_stream_generation.clone(),
    )?;
    let bus = playbin
        .bus()
        .ok_or_else(|| PlaybackError::Backend("GStreamer: no bus".into()))?;
    let mut stream_tags = (None::<String>, None::<String>);
    bus.add_watch(move |_, msg| {
        use gst::MessageView;
        match msg.view() {
            MessageView::Eos(_) => {
                if crossfading.load(Ordering::SeqCst) {
                    tracing::debug!(
                        "end-of-stream on the outgoing pipeline during a crossfade; \
                         suppressing TrackFinished (promotion drives the advance)"
                    );
                } else {
                    tracing::debug!("playback reached end-of-stream");
                    (*on_event)(PlayerEvent::TrackFinished);
                }
            }
            MessageView::StreamStart(_) => {
                cava_stream_generation.fetch_add(1, Ordering::AcqRel);
                // Fires on every stream start; only a gapless handoff (flagged
                // by the `about-to-finish` handler) turns into `AdvancedToNext`.
                stream_tags = (None, None);
                note_stream_start(&handoff_pending, on_event.as_ref());
            }
            MessageView::Tag(message) => {
                let tags = message.tags();
                let title = tags
                    .get::<gst::tags::Title>()
                    .map(|value| value.get().to_string());
                let organization = tags
                    .get::<gst::tags::Organization>()
                    .map(|value| value.get().to_string());
                if let Some(next) = merge_stream_tags(&stream_tags, title, organization) {
                    stream_tags = next.clone();
                    (*on_event)(PlayerEvent::StreamTags {
                        title: next.0,
                        organization: next.1,
                    });
                }
            }
            MessageView::Error(e) => {
                tracing::error!(error = %e.error(), debug = ?e.debug(), "GStreamer bus error");
                (*on_event)(PlayerEvent::Error(e.error().to_string()));
            }
            _ => {}
        }
        gst::glib::ControlFlow::Continue
    })
    .map_err(|e| PlaybackError::Backend(format!("GStreamer: {e}")))
}
