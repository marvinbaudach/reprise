package de.reprise.spike

import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import de.reprise.spike.scene.SceneState
import de.reprise.spike.scene.SpectrogramFrames
import kotlinx.coroutines.delay

internal const val PAUSED_SCENE_FRAME_INTERVAL_MS = 50L

internal fun interface SceneClock {
    fun nowNanos(): Long
}

internal fun interface ScenePositionSource {
    fun current(): ScenePositionSample
}

internal data class ScenePositionSample(
    val positionMs: Long,
    val observedAtNanos: Long,
    val playing: Boolean,
)

/** One callback on every allowed scene frame; non-null bands mean a new analysis frame. */
internal fun interface SceneFrameSink {
    fun onFrame(bands: FloatArray?)
}

/**
 * Converts position estimates to whole spectrogram indices and delegates all
 * signal evolution to [SceneState]. The same monotonic clock supplies elapsed
 * wall time for the fog's signal-independent base drift.
 */
internal class SceneDriver(
    private val frames: SpectrogramFrames,
    private val state: SceneState,
    private val clock: SceneClock,
    private val positionSource: ScenePositionSource,
    private val frameSink: SceneFrameSink? = null,
    private val framesAllowed: () -> Boolean,
) {
    var lastDrivenFrameIndex: Int? = null
        private set
    private var framesWithheld = false
    private var lastTickNanos: Long? = null
    private var firstTickNanos: Long? = null

    fun tick(): Boolean {
        if (!framesAllowed()) {
            noteFramesWithheld()
            return false
        }
        val sample = positionSource.current()
        val nowNanos = clock.nowNanos()
        val positionMs = estimatedPositionMs(sample, nowNanos)
        val frameIndex = frames.frameIndexFor(positionMs)
        val newFrame = lastDrivenFrameIndex != frameIndex
        val before = state.revision
        val analysed = frames.frameCount > 0
        if (analysed) {
            state.advanceTo(frameIndex, afterMissedFrames = framesWithheld)
        }
        val previousTick = lastTickNanos
        if (previousTick != null) {
            val elapsedSeconds = (nowNanos - previousTick).coerceAtLeast(0) / NANOS_PER_SECOND
            if (analysed) {
                state.advanceFogBy(elapsedSeconds)
            } else {
                // No spectrogram ever arrived for this track, so there is no
                // signal to follow. Wandering is the honest substitute: it is
                // visibly alive without pretending to answer music it cannot
                // hear.
                val totalSeconds = (nowNanos - (firstTickNanos ?: nowNanos)) / NANOS_PER_SECOND
                state.wanderTo(totalSeconds, elapsedSeconds)
            }
        }
        if (firstTickNanos == null) {
            firstTickNanos = nowNanos
        }
        lastTickNanos = nowNanos
        framesWithheld = false
        lastDrivenFrameIndex = frameIndex
        frameSink?.onFrame(
            when {
                !newFrame -> null
                analysed -> state.motionBands
                else -> EMPTY_BANDS
            },
        )
        return state.revision != before || frameSink != null
    }

    /**
     * Records that the power gate withheld frames while playback carried on.
     *
     * The driver notices this itself whenever [tick] is called through a closed
     * gate, but the frame loop is usually torn down instead of ticked, so the
     * caller that stops looping has to say so. Either way the next tick knows
     * its jump is a gap rather than a seek — the one thing an index delta
     * cannot tell on its own.
     */
    fun noteFramesWithheld() {
        framesWithheld = true
        lastTickNanos = null
    }

    private fun estimatedPositionMs(sample: ScenePositionSample, nowNanos: Long): Long {
        if (!sample.playing) return sample.positionMs.coerceAtLeast(0)
        val elapsedNanos = (nowNanos - sample.observedAtNanos).coerceAtLeast(0)
        val elapsedMs = (elapsedNanos / NANOS_PER_MILLISECOND)
            .coerceAtMost(measuredPositionIntervalMs)
        return sample.positionMs.coerceAtLeast(0) + elapsedMs
    }

    companion object {
        /** Measured from Media3PlaybackPort's published position interval. */
        const val measuredPositionIntervalMs = 500L
        private const val NANOS_PER_MILLISECOND = 1_000_000L
        private const val NANOS_PER_SECOND = 1_000_000_000f
        private val EMPTY_BANDS = FloatArray(0)
    }
}

private class MutableScenePositionSource : ScenePositionSource {
    private var sample = ScenePositionSample(0, 0, false)

    fun update(next: ScenePositionSample) {
        sample = next
    }

    override fun current(): ScenePositionSample = sample
}

private object SystemSceneClock : SceneClock {
    override fun nowNanos(): Long = System.nanoTime()
}

/**
 * Drives one scene and returns the draw revision Compose should observe.
 *
 * The loop runs at display cadence while playing and at no more than 20 Hz
 * while paused. Activity, screen and system-animation gates stop it entirely.
 */
@Composable
internal fun DriveScene(
    frames: SpectrogramFrames,
    state: SceneState,
    playback: PlaybackUiState,
    controller: AmbientMotionController,
    frameSink: SceneFrameSink? = null,
): Int {
    val source = remember(state) { MutableScenePositionSource() }
    val driver = remember(frames, state, frameSink) {
        SceneDriver(frames, state, SystemSceneClock, source, frameSink) {
            controller.sceneFramesAllowed
        }
    }
    var drawRevision by remember(state) { mutableIntStateOf(0) }
    val positionSample = remember(state, playback.positionMs, playback.isPlaying) {
        ScenePositionSample(
            positionMs = playback.positionMs,
            observedAtNanos = SystemSceneClock.nowNanos(),
            playing = playback.isPlaying,
        )
    }
    SideEffect {
        source.update(positionSample)
    }
    DisposableEffect(controller) {
        controller.attach()
        onDispose { controller.detach() }
    }

    val runtimeActive = controller.sceneFramesAllowed
    val animationsEnabled = controller.sceneAnimationsEnabled
    LaunchedEffect(
        driver,
        runtimeActive,
        animationsEnabled,
        playback.isPlaying,
        playback.positionMs,
    ) {
        if (!runtimeActive || !animationsEnabled) {
            driver.noteFramesWithheld()
            return@LaunchedEffect
        }
        if (frames.frameCount == 0 && frameSink == null) return@LaunchedEffect
        do {
            if (!playback.isPlaying) delay(PAUSED_SCENE_FRAME_INTERVAL_MS)
            withFrameNanos {
                if (driver.tick()) drawRevision += 1
            }
        } while (animationsEnabled)
    }
    return drawRevision
}
