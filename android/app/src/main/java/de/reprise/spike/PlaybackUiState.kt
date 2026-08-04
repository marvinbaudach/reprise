package de.reprise.spike

import uniffi.reprise_android_ffi.AndroidPlaybackSnapshot
import uniffi.reprise_android_ffi.AndroidPlaybackState
import uniffi.reprise_android_ffi.AndroidRepeatMode

internal data class PlaybackUiState(
    val ready: Boolean = false,
    val state: AndroidPlaybackState = AndroidPlaybackState.STOPPED,
    val currentIndex: Int? = null,
    val positionMs: Long = 0,
    val durationMs: Long = 0,
    val playPauseLabel: String = "Play",
    val shuffled: Boolean = false,
    val repeat: AndroidRepeatMode = AndroidRepeatMode.OFF,
    val error: String? = null,
)

internal val PlaybackUiState.isPlaying: Boolean
    get() = state == AndroidPlaybackState.PLAYING

internal val PlaybackUiState.progressFraction: Float
    get() = if (durationMs > 0) {
        (positionMs.toDouble() / durationMs.toDouble()).coerceIn(0.0, 1.0).toFloat()
    } else {
        0f
    }

internal fun AndroidPlaybackSnapshot.toUiState(): PlaybackUiState = PlaybackUiState(
    ready = true,
    state = state,
    currentIndex = currentIndex?.toInt(),
    positionMs = positionMs,
    durationMs = durationMs,
    playPauseLabel = if (state == AndroidPlaybackState.PLAYING) "Pause" else "Play",
    shuffled = shuffled,
    repeat = repeat,
    error = error,
)
