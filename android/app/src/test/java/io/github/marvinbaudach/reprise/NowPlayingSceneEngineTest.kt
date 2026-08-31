package io.github.marvinbaudach.reprise

import androidx.activity.ComponentActivity
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import io.github.marvinbaudach.reprise.ui.theme.RepriseTheme
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.reprise_android_ffi.AndroidColorScheme
import uniffi.reprise_android_ffi.AndroidPlaybackState

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36], qualifiers = "w500dp-h1000dp")
class NowPlayingSceneEngineTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    fun theSceneEngineExistsWhileTheScreenIsUpWithTheCoverShowing() {
        val factory = RecordingSceneEngineFactory()
        val controller = AmbientMotionController()
        val surfaceState = MobileSurfaceViewModel()
        compose.mainClock.autoAdvance = false
        compose.setContent { CoverScene(factory, controller, surfaceState) }
        compose.runOnIdle {
            controller.runtimeChanged(
                resumed = true,
                screenInteractive = true,
                animationsEnabled = true,
            )
        }

        compose.mainClock.advanceTimeBy(DISPLAY_FRAME_MS * 4)

        assertEquals(1, factory.created)
        assertTrue("the frame sink never reached DriveScene", factory.engine.ticks > 0)
    }

    @Test
    fun theCoverArmDoesNotBuildASceneItNeverDraws() {
        val factory = RecordingSceneEngineFactory()
        val surfaceState = MobileSurfaceViewModel()

        compose.setContent {
            CoverScene(factory, AmbientMotionController(), surfaceState)
        }
        compose.waitForIdle()
        compose.onNodeWithTag("now-playing-scene").captureToImage()

        assertEquals(0, factory.engine.sceneCalls)
    }

    @Composable
    private fun CoverScene(
        factory: RecordingSceneEngineFactory,
        controller: AmbientMotionController,
        surfaceState: MobileSurfaceViewModel,
    ) {
        val theme = MobileThemeSelection(
            palette = MobileTheme.NOCTURNE,
            colorScheme = AndroidColorScheme.SYSTEM,
            dynamicAvailable = false,
        )
        RepriseTheme(theme, darkPalette = true) {
            CompositionLocalProvider(
                LocalAmbientMotionController provides controller,
                LocalVisualSceneEngineFactory provides factory,
            ) {
                NowPlayingScene(
                    track = sceneEngineTrack(),
                    playback = PlaybackUiState(state = AndroidPlaybackState.PLAYING),
                    surfaceState = surfaceState,
                    visualizerOpacity = 0f,
                )
            }
        }
    }
}

private class RecordingSceneEngineFactory : VisualSceneEngineFactory {
    val engine = RecordingSceneEngine()
    var created = 0
        private set

    override fun create(): VisualSceneEngine {
        created += 1
        return engine
    }
}

private class RecordingSceneEngine : VisualSceneEngine {
    var ticks = 0
        private set
    var sceneCalls = 0
        private set

    override fun setAccent(red: Float, green: Float, blue: Float) = Unit
    override fun setPlaying(playing: Boolean) = Unit
    override fun noteTrackChanged() = Unit
    override fun ingestBands(bands: FloatArray) = Unit
    override fun tick() {
        ticks += 1
    }
    override fun scene(width: Float, height: Float): List<Float> {
        sceneCalls += 1
        return emptyList()
    }
    override fun close() = Unit
}

private fun sceneEngineTrack() = LibraryTrack(
    id = 17,
    uri = "content://provider/song.flac",
    title = "Song",
    artist = "Artist",
    album = "Album",
    durationMs = 180_000,
    playCount = 0,
    rating = 0,
)

private const val DISPLAY_FRAME_MS = 16L
