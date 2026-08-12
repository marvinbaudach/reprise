package de.reprise.spike

import androidx.activity.ComponentActivity
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import de.reprise.spike.scene.SceneState
import de.reprise.spike.scene.SpectrogramFrames
import java.util.concurrent.atomic.AtomicInteger
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
}

private const val DISPLAY_FRAME_MS = 16L
