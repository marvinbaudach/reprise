package io.github.marvinbaudach.reprise

import androidx.activity.ComponentActivity
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import io.github.marvinbaudach.reprise.scene.SpectrogramFrames
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

    @Test
    fun the_live_panel_still_scenes_its_bars_mid_swipe_off_centre() {
        // Bars used to fade with `near`, the panel's distance from the pager's centre — so the
        // panel that just became live could lose its bars again the instant a swipe carried it
        // away from dead centre, even though it is the one panel guaranteed to have something to
        // scene once PCM starts arriving. Neighbours are locked onto NativeVisualSceneEngineFactory
        // (see only_the_current_panel_uses_the_live_audio_scene_factory), so the live slot is the
        // only place this recording factory can observe the real call site.
        val factory = RecordingSceneEngineFactory()
        val analysis = ReadySpectrogramAnalysis()
        val surfaceState = MobileSurfaceViewModel()
        var positionPx by mutableStateOf(0f)

        compose.setContent {
            SwipeScene(factory, analysis, surfaceState, positionPx)
        }
        compose.waitForIdle()

        val sceneNode = compose.onNodeWithTag("now-playing-scene")
        val widthPx = sceneNode.fetchSemanticsNode().size.width.toFloat()
        positionPx = widthPx * 1.2f
        compose.waitForIdle()
        // Canvas draw lambdas only run on an actual draw pass; captureToImage forces one, the same
        // way theCoverArmDoesNotBuildASceneItNeverDraws does above to observe the opposite outcome.
        sceneNode.captureToImage()

        assertTrue(
            "the live panel must scene its bars even off-centre",
            factory.engine.sceneCalls > 0,
        )
    }

    @Test
    fun the_live_panel_keeps_polling_for_its_first_scene_when_nothing_was_ever_analysed() {
        // Regression: `hasVisualData`/the bars Canvas gate must not deadlock. A live panel with no
        // stored spectrogram starts with `panelHasVisualData == false`, and that Canvas is the only
        // place `sceneBytes()` — and therefore FrozenSceneBytes.hasCapturedScene — ever gets read.
        // Without `panelAwaitsFirstLiveScene` keeping the Canvas alive at zero opacity, this panel
        // could never learn that a real frame had landed, and its cover would stay up forever
        // instead of just until live audio arrives.
        val factory = RecordingSceneEngineFactory()
        val analysis = UnanalysedSpectrogramAnalysis()
        val surfaceState = MobileSurfaceViewModel()

        compose.setContent {
            SwipeScene(factory, analysis, surfaceState, positionPx = 0f)
        }
        compose.waitForIdle()
        // Canvas draw lambdas only run on an actual draw pass; captureToImage forces one, the same
        // way theCoverArmDoesNotBuildASceneItNeverDraws does above to observe the opposite outcome.
        compose.onNodeWithTag("now-playing-scene").captureToImage()

        assertTrue(
            "the live panel must keep polling for its first scene, not sit dark forever",
            factory.engine.sceneCalls > 0,
        )
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

    @Composable
    private fun SwipeScene(
        factory: RecordingSceneEngineFactory,
        analysis: TrackAnalysisPort,
        surfaceState: MobileSurfaceViewModel,
        positionPx: Float,
    ) {
        val theme = MobileThemeSelection(
            palette = MobileTheme.NOCTURNE,
            colorScheme = AndroidColorScheme.SYSTEM,
            dynamicAvailable = false,
        )
        RepriseTheme(theme, darkPalette = true) {
            CompositionLocalProvider(
                LocalAmbientMotionController provides AmbientMotionController(),
                LocalVisualSceneEngineFactory provides factory,
                LocalTrackAnalysis provides analysis,
            ) {
                val track = sceneEngineTrack()
                NowPlayingScene(
                    track = track,
                    playback = PlaybackUiState(state = AndroidPlaybackState.PLAYING),
                    surfaceState = surfaceState,
                    positionPx = positionPx,
                    currentIndex = 0,
                    panels = listOf(PlayPanel(0, track)),
                    visualizerOpacity = 1f,
                )
            }
        }
    }
}

private class ReadySpectrogramAnalysis : TrackAnalysisPort {
    override var revision by mutableLongStateOf(0L)
        private set

    override fun prepare(trackId: Long) = Unit

    override fun loadBars(trackId: Long, count: Int, deliver: (List<SpectralBar>?) -> Unit) =
        deliver(null)

    override fun loadSpectrogram(trackId: Long, deliver: (SpectrogramFrames?) -> Unit) =
        deliver(
            SpectrogramFrames(
                bandCount = 24,
                frameRateHz = 20,
                cells = ByteArray(24 * 4) { 128.toByte() },
            ),
        )
}

/** A track the desktop never analysed: a spectrogram with zero frames, delivered explicitly. */
private class UnanalysedSpectrogramAnalysis : TrackAnalysisPort {
    override var revision by mutableLongStateOf(0L)
        private set

    override fun prepare(trackId: Long) = Unit

    override fun loadBars(trackId: Long, count: Int, deliver: (List<SpectralBar>?) -> Unit) =
        deliver(null)

    override fun loadSpectrogram(trackId: Long, deliver: (SpectrogramFrames?) -> Unit) =
        deliver(SpectrogramFrames(bandCount = 24, frameRateHz = 20, cells = ByteArray(0)))
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
