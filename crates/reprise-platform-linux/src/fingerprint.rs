//! In-process GStreamer/Chromaprint fingerprinting for Linux.

use std::path::Path;

use gst::prelude::*;
use gstreamer as gst;
use reprise_core::fingerprint::{
    Fingerprint, FingerprintBackend, FingerprintCapability, FingerprintControl, FingerprintError,
    FingerprintOutcome, FingerprintProgress, GST_CHROMAPRINT_PIPELINE_REVISION,
};

const BUS_POLL_INTERVAL: gst::ClockTime = gst::ClockTime::from_mseconds(50);
const FINGERPRINT_SECONDS: u32 = 120;
// `identity` emits EOS before forwarding buffer N. One-second buffers make
// 121 the first 120 seconds of decoded audio.
const EOS_AFTER_BUFFER: i32 = 121;

pub(crate) const REQUIRED_ELEMENTS: [&str; 8] = [
    "uridecodebin",
    "audioconvert",
    "audioresample",
    "capsfilter",
    "audiobuffersplit",
    "identity",
    "chromaprint",
    "fakesink",
];

#[derive(Clone, Copy, Debug, Default)]
pub struct GstreamerFingerprintBackend;

pub(crate) fn cache_namespace(plugin_version: &str) -> String {
    let mut sanitized = String::with_capacity(plugin_version.len());
    let mut previous_was_separator = false;
    for character in plugin_version.chars() {
        if character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-') {
            sanitized.push(character);
            previous_was_separator = false;
        } else if !previous_was_separator {
            sanitized.push('-');
            previous_was_separator = true;
        }
    }
    let sanitized = sanitized.trim_matches('-');
    let version = if sanitized.is_empty() {
        "unknown"
    } else {
        sanitized
    };
    format!("gst-chromaprint-plugin-{version}-{GST_CHROMAPRINT_PIPELINE_REVISION}")
}

pub(crate) fn capability_with(
    initialize: impl FnOnce() -> Result<(), String>,
    factory_exists: impl Fn(&str) -> bool,
    chromaprint_plugin_version: impl FnOnce() -> Option<String>,
) -> FingerprintCapability {
    if let Err(detail) = initialize() {
        return FingerprintCapability::BackendInitFailed { detail };
    }
    let elements = REQUIRED_ELEMENTS
        .iter()
        .filter(|element| !factory_exists(element))
        .map(|element| (*element).to_string())
        .collect::<Vec<_>>();
    if !elements.is_empty() {
        return FingerprintCapability::MissingPlugin { elements };
    }
    let Some(plugin_version) =
        chromaprint_plugin_version().filter(|plugin_version| !plugin_version.trim().is_empty())
    else {
        return FingerprintCapability::BackendInitFailed {
            detail: "chromaprint plugin version unavailable".into(),
        };
    };
    FingerprintCapability::Available {
        cache_namespace: cache_namespace(&plugin_version),
    }
}

fn host_capability() -> FingerprintCapability {
    capability_with(
        || gst::init().map_err(|error| error.to_string()),
        |element| gst::ElementFactory::find(element).is_some(),
        || {
            gst::ElementFactory::find("chromaprint")?
                .plugin()
                .map(|plugin| plugin.version().to_string())
        },
    )
}

pub(crate) fn pipeline_description(identity_sleep_microseconds: u64) -> String {
    format!(
        "uridecodebin name=decoder ! audioconvert ! audioresample ! \
         audio/x-raw,format=S16LE,channels=1,rate=11025 ! \
         audiobuffersplit output-buffer-duration=1/1 ! \
         identity name=limit eos-after={EOS_AFTER_BUFFER} \
         sleep-time={identity_sleep_microseconds} ! \
         chromaprint name=fingerprinter duration={FINGERPRINT_SECONDS} ! \
         fakesink sync=false"
    )
}

struct PipelineNullGuard {
    pipeline: gst::Pipeline,
}

impl PipelineNullGuard {
    fn new(pipeline: gst::Pipeline) -> Self {
        Self { pipeline }
    }
}

impl Drop for PipelineNullGuard {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}

impl GstreamerFingerprintBackend {
    fn run(
        self,
        path: &Path,
        identity_sleep_microseconds: u64,
        progress: &mut dyn FnMut(FingerprintProgress) -> FingerprintControl,
    ) -> Result<FingerprintOutcome, FingerprintError> {
        if !path.is_file() {
            return Err(FingerprintError::FileNotFound(path.to_path_buf()));
        }
        if progress(FingerprintProgress {
            processed_seconds: 0,
            duration_seconds: None,
        }) == FingerprintControl::Cancel
        {
            return Ok(FingerprintOutcome::Cancelled);
        }

        let cache_namespace = match self.capability() {
            FingerprintCapability::Available { cache_namespace } => cache_namespace,
            capability => return Err(FingerprintError::BackendUnavailable(capability)),
        };

        let pipeline = gst::parse::launch(&pipeline_description(identity_sleep_microseconds))
            .map_err(|error| FingerprintError::DecodeFailed(error.to_string()))?
            .downcast::<gst::Pipeline>()
            .map_err(|_| {
                FingerprintError::DecodeFailed("backend did not create a pipeline".into())
            })?;
        let uri = gst::glib::filename_to_uri(path, None)
            .map_err(|error| FingerprintError::DecodeFailed(error.to_string()))?;
        pipeline
            .by_name("decoder")
            .ok_or_else(|| FingerprintError::DecodeFailed("pipeline has no decoder".into()))?
            .set_property("uri", uri);
        let guard = PipelineNullGuard::new(pipeline);
        guard
            .pipeline
            .set_state(gst::State::Playing)
            .map_err(|error| FingerprintError::DecodeFailed(error.to_string()))?;
        let bus = guard
            .pipeline
            .bus()
            .ok_or_else(|| FingerprintError::DecodeFailed("pipeline has no bus".into()))?;

        let duration = loop {
            let message = bus.timed_pop_filtered(
                BUS_POLL_INTERVAL,
                &[gst::MessageType::Eos, gst::MessageType::Error],
            );
            let duration = guard
                .pipeline
                .query_duration::<gst::ClockTime>()
                .map(gst::ClockTime::seconds);
            let processed_seconds = guard
                .pipeline
                .query_position::<gst::ClockTime>()
                .map_or(0, gst::ClockTime::seconds);

            match message.as_ref().map(|message| message.view()) {
                Some(gst::MessageView::Eos(_)) => {
                    break duration.ok_or(FingerprintError::DurationUnavailable)?;
                }
                Some(gst::MessageView::Error(error)) => {
                    return Err(FingerprintError::DecodeFailed(format!(
                        "{} ({:?})",
                        error.error(),
                        error.debug()
                    )));
                }
                _ => {
                    if progress(FingerprintProgress {
                        processed_seconds,
                        duration_seconds: duration,
                    }) == FingerprintControl::Cancel
                    {
                        return Ok(FingerprintOutcome::Cancelled);
                    }
                }
            }
        };

        let encoded = guard
            .pipeline
            .by_name("fingerprinter")
            .ok_or_else(|| FingerprintError::DecodeFailed("pipeline has no fingerprinter".into()))?
            .property::<Option<String>>("fingerprint")
            .filter(|fingerprint| !fingerprint.is_empty())
            .ok_or(FingerprintError::EmptyFingerprint)?;
        Ok(FingerprintOutcome::Completed(Fingerprint {
            encoded,
            duration_seconds: duration,
            cache_namespace,
        }))
    }

    #[cfg(test)]
    pub(crate) fn fingerprint_with_identity_sleep(
        self,
        path: &Path,
        identity_sleep_microseconds: u64,
        progress: &mut dyn FnMut(FingerprintProgress) -> FingerprintControl,
    ) -> Result<FingerprintOutcome, FingerprintError> {
        self.run(path, identity_sleep_microseconds, progress)
    }
}

impl FingerprintBackend for GstreamerFingerprintBackend {
    fn capability(&self) -> FingerprintCapability {
        host_capability()
    }

    fn fingerprint(
        &self,
        path: &Path,
        progress: &mut dyn FnMut(FingerprintProgress) -> FingerprintControl,
    ) -> Result<FingerprintOutcome, FingerprintError> {
        self.run(path, 0, progress)
    }
}

#[cfg(test)]
pub(crate) fn set_null_on_drop_for_test(pipeline: gst::Pipeline) {
    drop(PipelineNullGuard::new(pipeline));
}
