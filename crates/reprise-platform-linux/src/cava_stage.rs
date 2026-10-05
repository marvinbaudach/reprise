//! The CAVA processor and bass detector behind the pipeline's PCM tap, with
//! the decision of how each buffer restarts them.
//!
//! Kept free of GStreamer so the restart policy can be driven buffer by buffer
//! in a test, with the cadence the real tap delivers.

use reprise_core::playback::{
    BassPressureDetector, CavaBarProcessor, CavaConfig, CavaError, SpectrumFrame,
    SPECTRUM_BAND_COUNT,
};

/// How the processor restarts before the next buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Restart {
    /// The same stream continues.
    None,
    /// The visualizer was off, or the stage is new: nothing on screen
    /// continues, so the whole bar history clears.
    Hard,
    /// A new track or a seek: another stream's samples leave the FFT window but
    /// the shape on screen falls through gravity. The sensitivity is measured
    /// again from the new audio either way (AC-29).
    Stream,
}

/// Which restart the next buffer needs.
pub(crate) fn restart_for(
    was_enabled: bool,
    stream_generation_changed: bool,
    discontinuity: bool,
) -> Restart {
    if !was_enabled {
        Restart::Hard
    } else if stream_generation_changed || discontinuity {
        Restart::Stream
    } else {
        Restart::None
    }
}

pub(crate) struct CavaStage {
    processor: CavaBarProcessor,
    pressure_detector: BassPressureDetector,
    was_enabled: bool,
    seen_stream_generation: u64,
}

impl CavaStage {
    pub(crate) fn new(sample_rate_hz: u32, stream_generation: u64) -> Result<Self, CavaError> {
        Ok(Self {
            processor: CavaBarProcessor::new(CavaConfig::new(sample_rate_hz, SPECTRUM_BAND_COUNT))?,
            // Measured from the same PCM, but deliberately outside CAVA: the
            // bars are auto-sensitivity-normalized and cannot say how loud the
            // bass really is.
            pressure_detector: BassPressureDetector::new(sample_rate_hz),
            was_enabled: false,
            seen_stream_generation: stream_generation,
        })
    }

    /// The visualizer is switched off; the next buffer after it starts over.
    pub(crate) fn disable(&mut self) {
        self.was_enabled = false;
    }

    /// Analyzes one buffer of mono PCM.
    pub(crate) fn analyze(
        &mut self,
        stream_generation: u64,
        discontinuity: bool,
        pcm: &[f32],
    ) -> SpectrumFrame {
        let restart = restart_for(
            self.was_enabled,
            stream_generation != self.seen_stream_generation,
            discontinuity,
        );
        match restart {
            Restart::Hard => self.processor.reset(),
            Restart::Stream => self.processor.reset_stream(),
            Restart::None => {}
        }
        if restart != Restart::None {
            self.pressure_detector.reset();
        }
        self.was_enabled = true;
        self.seen_stream_generation = stream_generation;
        let bands: [f32; SPECTRUM_BAND_COUNT] = self
            .processor
            .process(pcm)
            .try_into()
            .expect("the CAVA processor returns its configured bar count");
        let pressure = self.pressure_detector.observe(pcm);
        SpectrumFrame::from_cava_bars(bands).with_bass_pressure(pressure)
    }
}

#[cfg(test)]
#[path = "cava_stage_tests.rs"]
mod tests;
