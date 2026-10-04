use super::settings::ReplayGainMode;

pub const REFERENCE_LUFS: f64 = -18.0;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ReplayGainTags {
    pub track_gain_db: Option<f64>,
    pub track_peak: Option<f64>,
    pub album_gain_db: Option<f64>,
    pub album_peak: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeasuredLoudness {
    pub integrated_lufs: f64,
    pub true_peak: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GainInputs {
    pub mode: ReplayGainMode,
    pub tags: ReplayGainTags,
    pub measured: Option<MeasuredLoudness>,
    pub album_measured: Option<(f64, f64)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GainSource {
    Tag,
    Measured,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolvedGain {
    pub gain_db: f64,
    pub source: GainSource,
}

pub fn album_loudness(tracks: &[(f64, i64)]) -> Option<f64> {
    let (weighted_energy, total_duration) = tracks
        .iter()
        .filter(|(lufs, duration_ms)| lufs.is_finite() && *duration_ms > 0)
        .fold((0.0, 0_i64), |(energy, duration), (lufs, duration_ms)| {
            (
                energy + (*duration_ms as f64 * 10_f64.powf(*lufs / 10.0)),
                duration + duration_ms,
            )
        });
    (total_duration > 0).then(|| 10.0 * (weighted_energy / total_duration as f64).log10())
}

pub fn resolve_gain(inputs: GainInputs) -> ResolvedGain {
    if inputs.mode == ReplayGainMode::Off {
        return no_gain();
    }

    let track = || {
        inputs
            .tags
            .track_gain_db
            .map(|gain| resolved(gain, inputs.tags.track_peak, GainSource::Tag))
            .or_else(|| measured_gain(inputs.measured))
    };

    match inputs.mode {
        ReplayGainMode::Off => no_gain(),
        ReplayGainMode::Track => track().unwrap_or_else(no_gain),
        ReplayGainMode::Album => inputs
            .tags
            .album_gain_db
            .map(|gain| resolved(gain, inputs.tags.album_peak, GainSource::Tag))
            .or_else(|| {
                inputs
                    .album_measured
                    .filter(|(lufs, _)| lufs.is_finite())
                    .map(|(lufs, peak)| {
                        resolved(REFERENCE_LUFS - lufs, Some(peak), GainSource::Measured)
                    })
            })
            .or_else(track)
            .unwrap_or_else(no_gain),
    }
}

fn measured_gain(measured: Option<MeasuredLoudness>) -> Option<ResolvedGain> {
    measured
        .filter(|value| value.integrated_lufs.is_finite())
        .map(|value| {
            resolved(
                REFERENCE_LUFS - value.integrated_lufs,
                Some(value.true_peak),
                GainSource::Measured,
            )
        })
}

fn resolved(gain_db: f64, peak: Option<f64>, source: GainSource) -> ResolvedGain {
    let peak_cap = peak
        .filter(|peak| peak.is_finite() && *peak > 0.0)
        .map(|peak| -20.0 * peak.log10());
    ResolvedGain {
        gain_db: peak_cap.map_or(gain_db, |cap| gain_db.min(cap)),
        source,
    }
}

fn no_gain() -> ResolvedGain {
    ResolvedGain {
        gain_db: 0.0,
        source: GainSource::None,
    }
}

#[cfg(test)]
#[path = "loudness_tests.rs"]
mod loudness_tests;
