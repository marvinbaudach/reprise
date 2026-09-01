package io.github.marvinbaudach.reprise

import androidx.activity.ComponentActivity
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import io.github.marvinbaudach.reprise.scene.SceneState
import io.github.marvinbaudach.reprise.scene.SpectrogramFrames
import java.util.concurrent.atomic.AtomicInteger
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.reprise_android_ffi.AndroidPlaybackState

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class DriveSceneComposeTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    fun bufferingTransitionRestartsTheComposableLoopAtDisplayCadence() {
        val controller = AmbientMotionController()
        val frames = SpectrogramFrames(24, 20, ByteArray(0))
        val scene = SceneState(frames)
        val deliveredFrames = AtomicInteger()
        val sink = SceneFrameSink { deliveredFrames.incrementAndGet() }
        var playback by mutableStateOf(
            PlaybackUiState(state = AndroidPlaybackState.PAUSED),
        )
        compose.mainClock.autoAdvance = false
        compose.setContent {
            DriveScene(
                frames = frames,
                state = scene,
                playback = playback,
                controller = controller,
                frameSink = sink,
            )
        }
        compose.runOnIdle {
            controller.runtimeChanged(
                resumed = true,
                screenInteractive = true,
                animationsEnabled = true,
            )
        }
        compose.mainClock.advanceTimeBy(DISPLAY_FRAME_MS)

        compose.runOnIdle {
            playback = PlaybackUiState(state = AndroidPlaybackState.BUFFERING)
        }
        val beforeBuffering = deliveredFrames.get()
        compose.mainClock.advanceTimeBy(DISPLAY_FRAME_MS * 4)

        assertTrue(
            "buffering delivered only ${deliveredFrames.get() - beforeBuffering} scene frames",
            deliveredFrames.get() - beforeBuffering >= 3,
        )
    }

    @Test
    fun aStalePausedUiStateDoesNotThrottleWhileAudioIsFlowing() {
        val sink = RecordingLiveAudioSink(liveAudio = true)

        driveAfterPlaybackTransition(
            initial = PlaybackUiState(state = AndroidPlaybackState.PLAYING),
            target = PlaybackUiState(state = AndroidPlaybackState.PAUSED),
            frames = SpectrogramFrames(24, 20, ByteArray(24)),
            sink = sink,
        )

        assertTrue("the throttle never asked the audio sink", sink.liveAudioChecks.get() > 0)
        assertTrue("live audio took the paused interval", sink.deliveredFrames.get() > 0)
    }

    @Test
    fun noStoredSpectrogramDoesNotThrottleWhenTheSinkIsListening() {
        val sink = RecordingLiveAudioSink(liveAudio = true)

        driveAfterPlaybackTransition(
            initial = PlaybackUiState(state = AndroidPlaybackState.PLAYING),
            target = PlaybackUiState(state = AndroidPlaybackState.PAUSED),
            frames = SpectrogramFrames(24, 20, ByteArray(0)),
            sink = sink,
        )

        assertTrue("the throttle never asked the listening sink", sink.liveAudioChecks.get() > 0)
        assertTrue(
            "the absent spectrogram took the paused interval",
            sink.deliveredFrames.get() > 0,
        )
    }

    @Test
    fun liveAudioIsReadOncePerDeliveredSceneFrame() {
        val sink = RecordingLiveAudioSink(liveAudio = true)

        driveAfterPlaybackTransition(
            initial = PlaybackUiState(state = AndroidPlaybackState.PLAYING),
            target = PlaybackUiState(state = AndroidPlaybackState.PAUSED),
            frames = SpectrogramFrames(24, 20, ByteArray(0)),
            sink = sink,
        )
        sink.reset()
        compose.mainClock.advanceTimeBy(DISPLAY_FRAME_MS * 10)

        assertTrue("the scene loop delivered no frames", sink.deliveredFrames.get() > 0)
        assertTrue(
            "${sink.liveAudioChecks.get()} live-audio reads crossed " +
                "for ${sink.deliveredFrames.get()} delivered frames",
            sink.liveAudioChecks.get() <= sink.deliveredFrames.get() + 1,
        )
    }

    @Test
    fun aGenuinelyPausedScreenStillTakesTheCheapInterval() {
        val frames = SpectrogramFrames(24, 20, ByteArray(24))
        val sink = RecordingLiveAudioSink(liveAudio = false)
        driveAfterPlaybackTransition(
            initial = PlaybackUiState(state = AndroidPlaybackState.PLAYING),
            target = PlaybackUiState(state = AndroidPlaybackState.PAUSED),
            frames = frames,
            sink = sink,
        )

        assertTrue(
            "the throttle never asked whether audio was live",
            sink.liveAudioChecks.get() > 0,
        )
        assertEquals(0, sink.deliveredFrames.get())

        compose.mainClock.advanceTimeBy(DISPLAY_FRAME_MS * 4)

        assertTrue("the cheap interval never released a frame", sink.deliveredFrames.get() > 0)
    }

    @Test
    fun aPositionTickDoesNotRestartTheFrameLoop() {
        val controller = AmbientMotionController()
        val frames = SpectrogramFrames(24, 20, ByteArray(24))
        val state = SceneState(frames)
        val sink = RecordingLiveAudioSink(liveAudio = false)
        var playback by mutableStateOf(PlaybackUiState(state = AndroidPlaybackState.PLAYING))
        compose.mainClock.autoAdvance = false
        compose.setContent {
            DriveScene(
                frames = frames,
                state = state,
                playback = playback,
                controller = controller,
                frameSink = sink,
            )
        }
        resumeScene(controller)
        compose.mainClock.advanceTimeBy(DISPLAY_FRAME_MS)
        sink.reset()

        compose.runOnIdle {
            playback = PlaybackUiState(state = AndroidPlaybackState.PAUSED)
        }
        compose.mainClock.advanceTimeBy(1L)
        assertTrue(sink.liveAudioChecks.get() > 0)
        sink.reset()

        compose.runOnIdle { playback = playback.copy(positionMs = 500) }
        compose.mainClock.advanceTimeBy(1L)

        assertEquals(
            "the position tick restarted the frame loop",
            0,
            sink.liveAudioChecks.get(),
        )
    }

    private fun driveAfterPlaybackTransition(
        initial: PlaybackUiState,
        target: PlaybackUiState,
        frames: SpectrogramFrames,
        sink: RecordingLiveAudioSink,
    ) {
        val controller = AmbientMotionController()
        val state = SceneState(frames)
        var playback by mutableStateOf(initial)
        compose.mainClock.autoAdvance = false
        compose.setContent {
            DriveScene(
                frames = frames,
                state = state,
                playback = playback,
                controller = controller,
                frameSink = sink,
            )
        }
        resumeScene(controller)
        compose.mainClock.advanceTimeBy(DISPLAY_FRAME_MS)
        sink.reset()
        compose.runOnIdle { playback = target }

        compose.mainClock.advanceTimeBy(DISPLAY_FRAME_MS * 2)
    }

    private fun resumeScene(controller: AmbientMotionController) {
        compose.runOnIdle {
            controller.runtimeChanged(
                resumed = true,
                screenInteractive = true,
                animationsEnabled = true,
            )
        }
    }
}

private class RecordingLiveAudioSink(
    private val liveAudio: Boolean,
) : SceneFrameSink {
    val deliveredFrames = AtomicInteger()
    val liveAudioChecks = AtomicInteger()

    override fun hasLiveAudio(): Boolean {
        liveAudioChecks.incrementAndGet()
        return liveAudio
    }

    override fun onFrame(bands: FloatArray?) {
        deliveredFrames.incrementAndGet()
    }

    fun reset() {
        deliveredFrames.set(0)
        liveAudioChecks.set(0)
    }
}

private const val DISPLAY_FRAME_MS = 16L
