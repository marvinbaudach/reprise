package io.github.marvinbaudach.reprise

import uniffi.reprise_android_ffi.AndroidEqualizerSnapshot
import uniffi.reprise_android_ffi.AndroidPlaybackSettings

internal fun playbackSettingsUiState(
    stored: AndroidPlaybackSettings,
    snapshot: AndroidEqualizerSnapshot?,
    presets: List<EqualizerPresetUi>,
): PlaybackSettingsUiState = PlaybackSettingsUiState(
    equalizerEnabled = stored.equalizerEnabled,
    gaplessEnabled = stored.gaplessEnabled,
    equalizerBands = snapshot?.bands.orEmpty().map { band ->
        EqualizerBandUi(
            frequencyHz = band.frequencyHz,
            gainDb = band.gainDb,
            minimumGainDb = band.minimumGainDb,
            maximumGainDb = band.maximumGainDb,
        )
    },
    volumeKeyTrackSwitchEnabled = stored.volumeKeyTrackSwitchEnabled,
    equalizerCurve = stored.equalizerCurve.map { point ->
        EqualizerCurvePoint(point.frequencyHz, point.gainDb)
    },
    equalizerPresets = presets,
    // A snapshot that reports no equalizer is a session we have asked: saying
    // "start playback" there would be false while a track plays.
    equalizerBandsAbsence = if (snapshot != null && !snapshot.available) {
        EqualizerBandsAbsence.NO_EQUALIZER_ON_THIS_DEVICE
    } else {
        EqualizerBandsAbsence.NO_PLAYBACK_YET
    },
)
