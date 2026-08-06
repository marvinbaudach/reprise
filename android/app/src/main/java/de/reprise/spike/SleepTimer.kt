package de.reprise.spike

import android.os.Handler
import android.os.SystemClock
import kotlin.math.min
import uniffi.reprise_android_ffi.AndroidPlaybackSnapshot
import uniffi.reprise_android_ffi.AndroidPlaybackState

internal sealed interface SleepTimerSelection {
    data class Minutes(val value: Int) : SleepTimerSelection

    data object EndOfTrack : SleepTimerSelection
}

internal data class SleepTimerUiState(
    val active: Boolean = false,
    val selection: SleepTimerSelection? = null,
    val remainingSeconds: Long? = null,
    val fading: Boolean = false,
)

/** Service-owned wall-clock/end-of-track countdown and gentle fade. */
internal class SleepTimerController(
    private val handler: Handler,
    private val applyVolume: (Float) -> Unit,
    private val pause: () -> Unit,
    private val publish: (SleepTimerUiState) -> Unit,
    private val nowMs: () -> Long = SystemClock::elapsedRealtime,
) {
    private val pending = mutableListOf<Runnable>()
    private var state = SleepTimerUiState()
    private var deadlineMs: Long? = null
    private var endOfTrackId: Long? = null

    fun state(): SleepTimerUiState = state

    fun start(selection: SleepTimerSelection, playback: AndroidPlaybackSnapshot?) {
        cancelScheduled(restoreVolume = true)
        deadlineMs = when (selection) {
            is SleepTimerSelection.Minutes -> {
                require(selection.value in TIMER_MINUTES) { "unsupported sleep timer duration" }
                nowMs() + selection.value * 60_000L
            }
            SleepTimerSelection.EndOfTrack -> null
        }
        endOfTrackId = playback?.currentTrackId
        state = SleepTimerUiState(
            active = true,
            selection = selection,
            remainingSeconds = (selection as? SleepTimerSelection.Minutes)
                ?.value
                ?.times(60)
                ?.toLong(),
        )
        publish(state)
        when (selection) {
            is SleepTimerSelection.Minutes -> tickFixedTimer()
            SleepTimerSelection.EndOfTrack -> playback?.let(::onPlaybackSnapshot)
        }
    }

    fun cancel() {
        if (!state.active) return
        cancelScheduled(restoreVolume = true)
        finish()
    }

    fun close() {
        cancelScheduled(restoreVolume = true)
        state = SleepTimerUiState()
    }

    fun onPlaybackSnapshot(snapshot: AndroidPlaybackSnapshot) {
        if (!state.active || state.selection != SleepTimerSelection.EndOfTrack || state.fading) {
            return
        }
        val armedId = endOfTrackId
        if (armedId == null) {
            endOfTrackId = snapshot.currentTrackId
            return
        }
        val changedTrack = snapshot.currentTrackId != armedId
        val ended = snapshot.state == AndroidPlaybackState.STOPPED || snapshot.currentTrackId == null
        val fadeWindowReached = snapshot.state == AndroidPlaybackState.PLAYING &&
            snapshot.durationMs > 0 &&
            snapshot.durationMs - snapshot.positionMs <= FADE_DURATION_MS
        if (changedTrack || ended || fadeWindowReached) beginFade()
    }

    private fun tickFixedTimer() {
        val deadline = deadlineMs ?: return
        val remainingMs = deadline - nowMs()
        if (remainingMs <= 0) {
            beginFade()
            return
        }
        val seconds = (remainingMs + 999L) / 1_000L
        if (state.remainingSeconds != seconds) {
            state = state.copy(remainingSeconds = seconds)
            publish(state)
        }
        schedule(min(1_000L, remainingMs), ::tickFixedTimer)
    }

    private fun beginFade() {
        if (!state.active || state.fading) return
        cancelScheduled(restoreVolume = false)
        state = state.copy(remainingSeconds = 0, fading = true)
        publish(state)
        for (step in 1..FADE_STEPS) {
            schedule(step * FADE_STEP_MS) {
                applyVolume(1f - step.toFloat() / FADE_STEPS)
                if (step == FADE_STEPS) {
                    pause()
                    applyVolume(1f)
                    finish()
                }
            }
        }
    }

    private fun schedule(delayMs: Long, action: () -> Unit) {
        lateinit var runnable: Runnable
        runnable = Runnable {
            pending.remove(runnable)
            action()
        }
        pending += runnable
        handler.postDelayed(runnable, delayMs)
    }

    private fun cancelScheduled(restoreVolume: Boolean) {
        pending.forEach(handler::removeCallbacks)
        pending.clear()
        if (restoreVolume && state.fading) applyVolume(1f)
        deadlineMs = null
        endOfTrackId = null
    }

    private fun finish() {
        deadlineMs = null
        endOfTrackId = null
        state = SleepTimerUiState()
        publish(state)
    }

    internal companion object {
        val TIMER_MINUTES = listOf(15, 30, 45, 60)
        const val FADE_DURATION_MS = 4_000L
        private const val FADE_STEPS = 8
        private const val FADE_STEP_MS = FADE_DURATION_MS / FADE_STEPS
    }
}
