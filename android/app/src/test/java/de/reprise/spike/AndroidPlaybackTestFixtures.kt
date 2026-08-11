package de.reprise.spike

import uniffi.reprise_android_ffi.AndroidPlaybackSnapshot
import uniffi.reprise_android_ffi.AndroidPlaybackState
import uniffi.reprise_android_ffi.AndroidRepeatMode

internal fun m9bSnapshot(trackId: Long) = AndroidPlaybackSnapshot(
    state = AndroidPlaybackState.PLAYING,
    currentIndex = 0u,
    currentTrackId = trackId,
    currentTrackUri = "content://provider/document/$trackId.flac",
    positionMs = 12_000,
    durationMs = 120_000,
    automaticAdvanceCount = 0u,
    shuffled = false,
    repeat = AndroidRepeatMode.OFF,
    error = null,
)
