//! Audio-analysis backend composition and window-scoped runtime lifetime.

use std::path::Path;
use std::sync::Arc;

use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::audio_analysis::AudioAnalysisBackend;
use reprise_core::waveform::WaveformBackend;
use reprise_platform_linux::audio_analysis::GstreamerAudioAnalysisBackend;
use reprise_platform_linux::waveform::GstreamerWaveformBackend;

use super::scan::audio_analysis_runtime::AudioAnalysisRuntime;

pub(super) fn setup(
    db_path: &Path,
    window: &adw::ApplicationWindow,
    enabled: bool,
) -> (Arc<dyn WaveformBackend>, Option<AudioAnalysisRuntime>) {
    let waveform: Arc<dyn WaveformBackend> = Arc::new(GstreamerWaveformBackend);
    let analysis: Arc<dyn AudioAnalysisBackend> = Arc::new(GstreamerAudioAnalysisBackend);
    let runtime =
        match AudioAnalysisRuntime::new(db_path.to_path_buf(), analysis, waveform.clone(), enabled)
        {
            Ok(runtime) => Some(runtime),
            Err(error) => {
                tracing::error!(%error, "could not start audio analysis runtime");
                None
            }
        };
    if let Some(runtime) = &runtime {
        let runtime = runtime.clone();
        window.connect_close_request(move |_| {
            runtime.shutdown();
            gtk4::glib::Propagation::Proceed
        });
    }
    (waveform, runtime)
}
