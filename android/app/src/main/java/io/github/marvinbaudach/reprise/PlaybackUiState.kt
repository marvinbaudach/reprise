package io.github.marvinbaudach.reprise

import androidx.compose.runtime.Immutable
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
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
    val faultNotice: TransientMessage? = null,
    val sleepTimer: SleepTimerUiState = SleepTimerUiState(),
)

@Immutable
internal data class LibraryPlayback(
    val currentIndex: Int? = null,
    val currentTrackId: Long? = null,
    val currentTrackUri: String? = null,
    val state: AndroidPlaybackState = AndroidPlaybackState.STOPPED,
    val error: String? = null,
    val faultNotice: TransientMessage? = null,
)

internal fun PlaybackUiState.libraryPlayback() = LibraryPlayback(
    currentIndex = currentIndex,
    currentTrackId = currentTrackId,
    currentTrackUri = currentTrackUri,
    state = state,
    error = error,
    faultNotice = faultNotice,
)

internal data class PlaybackFaultNoticeUpdate(
    val observedCount: ULong,
    val message: TransientMessage?,
)

internal fun updatePlaybackFaultNotice(
    previousCount: ULong?,
    currentCount: ULong,
    text: String?,
    previousMessage: TransientMessage?,
): PlaybackFaultNoticeUpdate {
    if (previousCount == null) {
        return PlaybackFaultNoticeUpdate(currentCount, null)
    }
    if (currentCount <= previousCount) {
        return PlaybackFaultNoticeUpdate(
            observedCount = currentCount,
            message = previousMessage.takeIf { text != null },
        )
    }
    return PlaybackFaultNoticeUpdate(
        observedCount = currentCount,
        message = text?.let { TransientMessage(it).after(previousMessage) },
    )
}

internal class PlaybackFaultNoticeObserver(
    private val scope: CoroutineScope,
    private val currentState: () -> PlaybackUiState,
    private val publish: (PlaybackUiState) -> Unit,
) {
    private var observedCount: ULong? = null

    fun accept(snapshot: AndroidPlaybackSnapshot) {
        val previous = currentState()
        val update = updatePlaybackFaultNotice(
            previousCount = observedCount,
            currentCount = snapshot.faultNoticeCount,
            text = snapshot.faultNotice,
            previousMessage = previous.faultNotice,
        )
        observedCount = update.observedCount
        publish(
            snapshot.toUiState().copy(
                faultNotice = update.message,
                sleepTimer = previous.sleepTimer,
            ),
        )
        val raised = update.message.takeIf { it != previous.faultNotice } ?: return
        scope.launch {
            delay(TRANSIENT_MESSAGE_MS)
            val current = currentState()
            if (current.faultNotice == raised) publish(current.copy(faultNotice = null))
        }
    }
}

internal val PlaybackUiState.isPlaying: Boolean
    get() = state == AndroidPlaybackState.PLAYING

internal val LibraryPlayback.isPlaying: Boolean
    get() = state == AndroidPlaybackState.PLAYING

/** Whether the play/pause control currently performs and presents Pause. */
internal val PlaybackUiState.playPauseShowsPause: Boolean
    get() = state.hasPlayIntent

internal val LibraryPlayback.playPauseShowsPause: Boolean
    get() = state.hasPlayIntent

internal val PlaybackUiState.playPauseSymbol: String
    get() = if (playPauseShowsPause) "pause" else "play_arrow"

internal val LibraryPlayback.playPauseSymbol: String
    get() = if (playPauseShowsPause) "pause" else "play_arrow"

internal val PlaybackUiState.playPauseLabel: String
    get() = if (playPauseShowsPause) "Pause" else "Play"

internal val LibraryPlayback.playPauseLabel: String
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
