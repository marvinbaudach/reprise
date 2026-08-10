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

/**
 * Converts position estimates to whole spectrogram indices and delegates all
 * signal evolution to [SceneState]. The clock is used only for interpolation;
 * it never enters the state mathematics.
 */
internal class SceneDriver(
    private val frames: SpectrogramFrames,
    private val state: SceneState,
    private val clock: SceneClock,
    private val positionSource: ScenePositionSource,
    private val framesAllowed: () -> Boolean,
) {
    var lastDrivenFrameIndex: Int? = null
        private set
    private var framesWithheld = false

    fun tick(): Boolean {
        if (!framesAllowed()) {
            noteFramesWithheld()
            return false
        }
        val sample = positionSource.current()
        val positionMs = estimatedPositionMs(sample, clock.nowNanos())
        val frameIndex = frames.frameIndexFor(positionMs)
        val before = state.revision
        state.advanceTo(frameIndex, afterMissedFrames = framesWithheld)
        framesWithheld = false
        lastDrivenFrameIndex = frameIndex
        return state.revision != before
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
 * Playing animation loops only while the activity and screen admit frames.
 * Paused or system-animation-off paths take one frame to adopt the current raw
 * signal and then stop scheduling callbacks.
 */
@Composable
internal fun DriveScene(
    frames: SpectrogramFrames,
    state: SceneState,
    playback: PlaybackUiState,
    controller: AmbientMotionController,
): Int {
    val source = remember(state) { MutableScenePositionSource() }
    val driver = remember(frames, state) {
        SceneDriver(frames, state, SystemSceneClock, source) { controller.sceneFramesAllowed }
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
        if (!runtimeActive) {
            driver.noteFramesWithheld()
            return@LaunchedEffect
        }
        if (frames.frameCount == 0) return@LaunchedEffect
        do {
            withFrameNanos {
                if (driver.tick()) drawRevision += 1
            }
        } while (animationsEnabled && playback.isPlaying)
    }
    return drawRevision
}
