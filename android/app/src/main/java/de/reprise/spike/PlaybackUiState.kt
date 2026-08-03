package de.reprise.spike

import uniffi.reprise_android_ffi.AndroidPlaybackSnapshot
import uniffi.reprise_android_ffi.AndroidPlaybackState

internal data class PlaybackUiState(
    val ready: Boolean = false,
    val state: AndroidPlaybackState = AndroidPlaybackState.STOPPED,
    val currentIndex: Int? = null,
    val positionReadout: String = "0:00 / --:--",
    val playPauseLabel: String = "Play",
    val error: String? = null,
)

internal fun AndroidPlaybackSnapshot.toUiState(): PlaybackUiState = PlaybackUiState(
    ready = true,
    state = state,
    currentIndex = currentIndex?.toInt(),
    positionReadout = buildString {
        append(formatDuration(positionMs))
        append(" / ")
        append(if (durationMs > 0) formatDuration(durationMs) else "--:--")
    },
    playPauseLabel = if (state == AndroidPlaybackState.PLAYING) "Pause" else "Play",
    error = error,
)
