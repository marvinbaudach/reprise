package de.reprise.spike

import uniffi.reprise_android_ffi.AndroidPlaybackSnapshot
import uniffi.reprise_android_ffi.AndroidPlaybackState
import uniffi.reprise_android_ffi.AndroidRepeatMode

internal data class PlaybackUiState(
    val ready: Boolean = false,
    val state: AndroidPlaybackState = AndroidPlaybackState.STOPPED,
    val currentIndex: Int? = null,
    val currentTrackId: Long? = null,
    val currentTrackUri: String? = null,
    val positionMs: Long = 0,
    val durationMs: Long = 0,
    val shuffled: Boolean = false,
    val repeat: AndroidRepeatMode = AndroidRepeatMode.OFF,
    val error: String? = null,
    val sleepTimer: SleepTimerUiState = SleepTimerUiState(),
)

internal val PlaybackUiState.isPlaying: Boolean
    get() = state == AndroidPlaybackState.PLAYING

/** Whether the play/pause control currently performs and presents Pause. */
internal val PlaybackUiState.playPauseShowsPause: Boolean
    get() = state.hasPlayIntent

internal val PlaybackUiState.playPauseSymbol: String
    get() = if (playPauseShowsPause) "pause" else "play_arrow"

internal val PlaybackUiState.playPauseLabel: String
    get() = if (playPauseShowsPause) "Pause" else "Play"

/** Playback intent used only by continuously animated visual presentation. */
internal val PlaybackUiState.visualizerActive: Boolean
    get() = state.hasPlayIntent

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
    currentTrackId = currentTrackId,
    currentTrackUri = currentTrackUri,
    positionMs = positionMs,
    durationMs = durationMs,
    shuffled = shuffled,
    repeat = repeat,
    error = error,
)

private val AndroidPlaybackState.hasPlayIntent: Boolean
    get() = this == AndroidPlaybackState.PLAYING || this == AndroidPlaybackState.BUFFERING
