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

/** One callback on every allowed scene frame; non-null bands mean a fallback analysis frame. */
internal fun interface SceneFrameSink {
    fun onFrame(bands: FloatArray?)

    fun hasLiveAudio(): Boolean = false

    fun bassPressure(): VisualBassPressure = VisualBassPressure.SILENT
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
    frameSink: SceneFrameSink? = null,
    private val framesAllowed: () -> Boolean,
) {
    private var frameSink = frameSink
    private var frameSinkNeedsSnapshot = frameSink != null
    var lastDrivenFrameIndex: Int? = null
        private set
    private var framesWithheld = false
    private var lastTickNanos: Long? = null
    private var firstTickNanos: Long? = null
    private var lastFrameFraction = 0f

    fun tick(): Boolean {
        if (!framesAllowed()) {
            noteFramesWithheld()
            return false
        }
        val nowNanos = clock.nowNanos()
        val sink = frameSink
        val liveAudio = sink?.hasLiveAudio() == true
        val before = state.revision
        val previousTick = lastTickNanos
        val elapsedSeconds = previousTick?.let { previous ->
            (nowNanos - previous).coerceAtLeast(0) / NANOS_PER_SECOND
        } ?: 0.0
        if (previousTick != null) {
            state.advanceShimmerBy(elapsedSeconds)
        }
        val outgoingBands = if (liveAudio) {
            state.adoptLiveBassPressure(sink.bassPressure(), elapsedSeconds.toFloat())
            lastDrivenFrameIndex = null
            null
        } else {
            fallbackBands(nowNanos, elapsedSeconds.toFloat())
        }
        if (firstTickNanos == null) {
            firstTickNanos = nowNanos
        }
        lastTickNanos = nowNanos
        framesWithheld = false
        sink?.onFrame(outgoingBands)
        if (sink != null && !liveAudio) frameSinkNeedsSnapshot = false
        return state.revision != before || sink != null
    }

    /** Reads the stored 20 Hz analysis only while decoded PCM is unavailable. */
    private fun fallbackBands(nowNanos: Long, elapsedSeconds: Float): FloatArray? {
        val sample = positionSource.current()
        val positionMs = estimatedPositionMs(sample, nowNanos)
        val frameIndex = frames.frameIndexFor(positionMs)
        val frameFraction = frames.frameFractionFor(positionMs)
        val newFrame = lastDrivenFrameIndex != frameIndex
        val analysed = frames.frameCount > 0
        if (analysed) {
            state.advanceTo(frameIndex, afterMissedFrames = framesWithheld)
            state.readBassPressureAt(frameFraction)
            state.advanceFogBy(elapsedSeconds)
        } else if (lastTickNanos != null) {
            // No PCM and no stored analysis: visibly alive without pretending
            // to answer music the app cannot hear.
            val totalSeconds = (nowNanos - (firstTickNanos ?: nowNanos)) / NANOS_PER_SECOND
            state.wanderTo(totalSeconds.toFloat(), elapsedSeconds)
        }
        lastDrivenFrameIndex = frameIndex
        val bands = bandsForTick(analysed, newFrame, frameFraction)
        lastFrameFraction = frameFraction
        return bands
    }

    /**
     * What this tick has to hand the visualizer, or null when it has nothing
     * new to say.
     *
     * A stepped frame is passed on as it is. The two ticks that fall between
     * two 20 Hz frames would otherwise repeat it verbatim and leave the picture
     * standing for ~33 ms each, so they read the followers at the point the
     * playhead has reached inside the frame instead. That reading only moves
     * when the playhead does: a paused position keeps its fraction, sends
     * nothing, and leaves the engine's own release to fade the bars out. Where
     * the analysis is already as fine as the display, every tick brings a frame
     * and this branch is never reached.
     */
    private fun bandsForTick(
        analysed: Boolean,
        newFrame: Boolean,
        frameFraction: Float,
    ): FloatArray? {
        if (!analysed) return if (frameSinkNeedsSnapshot) EMPTY_BANDS else null
        if (newFrame || frameSinkNeedsSnapshot) return state.motionBands
        if (frameFraction == lastFrameFraction) return null
        return state.motionBandsWithin(frameFraction)
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

    fun setFrameSink(sink: SceneFrameSink?) {
        if (sink != null && sink !== frameSink) frameSinkNeedsSnapshot = true
        frameSink = sink
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
        private const val NANOS_PER_SECOND = 1_000_000_000.0
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
    val driver = remember(frames, state) {
        SceneDriver(frames, state, SystemSceneClock, source) {
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
        driver.setFrameSink(frameSink)
    }
    DisposableEffect(controller) {
        controller.attach()
        onDispose { controller.detach() }
    }

    val runtimeActive = controller.sceneFramesAllowed
    val animationsEnabled = controller.sceneAnimationsEnabled
    val visualizerActive = playback.visualizerActive
    LaunchedEffect(
        driver,
        runtimeActive,
        animationsEnabled,
        visualizerActive,
        playback.positionMs,
        frameSink,
    ) {
        if (!runtimeActive || !animationsEnabled) {
            driver.noteFramesWithheld()
            return@LaunchedEffect
        }
        do {
            if (!visualizerActive || frames.frameCount == 0 && frameSink == null) {
                delay(PAUSED_SCENE_FRAME_INTERVAL_MS)
            }
            withFrameNanos {
                if (driver.tick()) drawRevision += 1
            }
        } while (animationsEnabled)
    }
    return drawRevision
}
