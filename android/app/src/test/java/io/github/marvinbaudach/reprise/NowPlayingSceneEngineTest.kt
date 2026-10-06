package io.github.marvinbaudach.reprise

import androidx.activity.ComponentActivity
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import io.github.marvinbaudach.reprise.scene.SpectrogramFrames
import io.github.marvinbaudach.reprise.ui.theme.RepriseTheme
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
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

    @Test
    fun a_neighbour_without_a_spectrogram_mirrors_the_live_scene_while_the_swipe_shows_it() {
        // In visualizer mode the neighbour used to show its cover for the whole
        // swipe: its own engine has no audio and no stored spectrogram, so
        // `panelHasVisualData` stayed false. It now reads the live panel's
        // engine, tinted in its own accent, for as long as it is on the screen.
        val factory = RecordingSceneEngineFactory()
        val analysis = UnanalysedSpectrogramAnalysis()
        val surfaceState = MobileSurfaceViewModel()
        var positionPx by mutableStateOf(0f)

        compose.setContent {
            SwipeScene(factory, analysis, surfaceState, positionPx, withNeighbour = true)
        }
        compose.waitForIdle()
        val sceneNode = compose.onNodeWithTag("now-playing-scene")
        sceneNode.captureToImage()
        assertEquals("at rest the neighbour is off the screen and mirrors nothing", 0, factory.engine.tintedCalls)

        val widthPx = sceneNode.fetchSemanticsNode().size.width.toFloat()
        positionPx = widthPx * 0.5f
        compose.waitForIdle()
        sceneNode.captureToImage()

        assertTrue(
            "the neighbour on the screen must draw the live scene in its own accent",
            factory.engine.tintedCalls > 0,
        )
    }

    @Test
    fun the_outgoing_panel_mirrors_the_new_live_engine_on_its_way_out() {
        // The panel that just lost the live slot has a picture of its own — the last scene it
        // drew while live, kept in FrozenSceneBytes across the engine swap. That picture is not
        // what slides out: bars are not track-identifiable, and a frozen frame next to a moving
        // one reads as a stutter, so the outgoing panel mirrors the engine that is now live,
        // tinted in its own accent, exactly like a neighbour that never had a picture. This is a
        // design choice, not an accident of the rule ignoring the captured scene.
        val factory = RecordingSceneEngineFactory(sceneRecord = FLAT_RECT_RECORD)
        val analysis = UnanalysedSpectrogramAnalysis()
        val surfaceState = MobileSurfaceViewModel()
        var positionPx by mutableStateOf(0f)
        var currentIndex by mutableStateOf(0)

        compose.setContent {
            SwipeScene(factory, analysis, surfaceState, positionPx, withNeighbour = true, currentIndex = currentIndex)
        }
        compose.waitForIdle()
        val sceneNode = compose.onNodeWithTag("now-playing-scene")
        sceneNode.captureToImage()
        assertTrue("the live panel captured a picture of its own first", factory.engine.sceneCalls > 0)
        assertEquals("at rest nothing mirrors", 0, factory.engine.tintedCalls)

        val widthPx = sceneNode.fetchSemanticsNode().size.width.toFloat()
        currentIndex = 1
        positionPx = widthPx * 0.5f
        compose.waitForIdle()
        sceneNode.captureToImage()

        // Panel 1 is live now and draws through sceneBytes; the only panel left to tint is the
        // outgoing panel 0, captured picture and all.
        assertTrue(
            "the outgoing panel must mirror the new live engine, not replay its frozen picture",
            factory.engine.tintedCalls > 0,
        )
    }

    @Test
    fun the_new_live_panel_adopts_the_outgoing_engines_bar_shape() {
        // A swipe hands the live slot to a brand-new engine (a fresh factory.create() call, see
        // visualSceneFactoryForPanel) that otherwise starts from zero for one frame. The panel
        // taking over the live slot must instead adopt the outgoing engine's bar shape — this is
        // the only place the recording factory can observe both engines the live slot passes
        // through, so it needs a distinct engine per create() call rather than the one shared
        // instance the other tests above rely on.
        val factory = RecordingSceneEngineFactory(distinctEngines = true)
        val analysis = UnanalysedSpectrogramAnalysis()
        val surfaceState = MobileSurfaceViewModel()
        var currentIndex by mutableStateOf(0)

        compose.setContent {
            SwipeScene(
                factory,
                analysis,
                surfaceState,
                positionPx = 0f,
                withNeighbour = true,
                currentIndex = currentIndex,
            )
        }
        compose.waitForIdle()
        val outgoing = factory.createdEngines[0]
        assertTrue(
            "the first live engine has no predecessor and must not have adopted anything",
            outgoing.adoptedShapes.isEmpty(),
        )
        outgoing.setCurrentBands(SEED_BANDS)

        currentIndex = 1
        compose.waitForIdle()

        val incoming = factory.createdEngines[1]
        assertArrayEquals(
            "the new live engine must adopt the outgoing engine's reported bands",
            SEED_BANDS,
            incoming.adoptedShapes.single(),
            0f,
        )
        assertEquals(
            "adoptShape must land after noteTrackChanged, or the reset wipes the adopted shape",
            listOf("noteTrackChanged", "adoptShape"),
            incoming.callSequence,
        )
    }

    @Test
    fun ac_29_the_new_live_panel_adopts_the_last_live_shape_not_the_decayed_display() {
        // The outgoing audio can stop, or the transport can blip, before the new panel composes
        // (the `slow-next` case). By then the engine's displayed bars have decayed toward the
        // resting shape; the panel taking over must adopt the last shape live audio drew instead.
        val factory = RecordingSceneEngineFactory(distinctEngines = true)
        val analysis = UnanalysedSpectrogramAnalysis()
        val surfaceState = MobileSurfaceViewModel()
        var currentIndex by mutableStateOf(0)

        compose.setContent {
            SwipeScene(
                factory,
                analysis,
                surfaceState,
                positionPx = 0f,
                withNeighbour = true,
                currentIndex = currentIndex,
            )
        }
        compose.waitForIdle()
        val outgoing = factory.createdEngines[0]
        outgoing.setCurrentBands(DECAYED_BANDS)
        outgoing.setAdoptableBands(SEED_BANDS)

        currentIndex = 1
        compose.waitForIdle()

        val incoming = factory.createdEngines[1]
        assertArrayEquals(
            "the new live engine must adopt the last live shape, not the decayed display",
            SEED_BANDS,
            incoming.adoptedShapes.single(),
            0f,
        )
    }

    @Test
    fun shouldAdoptLiveShapeOnlyForANewLivePanelWithADifferentPredecessor() {
        val previous = RecordingSceneEngine()
        val created = RecordingSceneEngine()

        assertTrue(shouldAdoptLiveShape(live = true, previous = previous, created = created))
        assertFalse(
            "a non-live panel never scenes live audio and must not adopt a shape",
            shouldAdoptLiveShape(live = false, previous = previous, created = created),
        )
        assertFalse(
            "a live panel with no predecessor has nothing to adopt",
            shouldAdoptLiveShape(live = true, previous = null, created = created),
        )
        assertFalse(
            "an engine must never adopt a shape from itself",
            shouldAdoptLiveShape(live = true, previous = created, created = created),
        )
    }

    @Test
    fun the_live_scene_handle_publishes_fog_and_state_while_live_and_clears_when_the_panel_leaves() {
        // liveScene is hoisted with a default (`remember { LiveSceneHandle() }`) precisely so a
        // test can hand in its own instance and read what the live panel published to it --
        // there is no factory to intercept here the way RecordingSceneEngineFactory reaches
        // liveScene.engine, since fog and state never went through an injectable factory.
        val handle = LiveSceneHandle()
        val surfaceState = MobileSurfaceViewModel()
        var showPanel by mutableStateOf(true)

        compose.setContent {
            val theme = MobileThemeSelection(
                palette = MobileTheme.NOCTURNE,
                colorScheme = AndroidColorScheme.SYSTEM,
                dynamicAvailable = false,
            )
            RepriseTheme(theme, darkPalette = true) {
                CompositionLocalProvider(
                    LocalAmbientMotionController provides AmbientMotionController(),
                    LocalVisualSceneEngineFactory provides RecordingSceneEngineFactory(),
                ) {
                    val track = sceneEngineTrack()
                    NowPlayingScene(
                        track = track,
                        playback = PlaybackUiState(state = AndroidPlaybackState.PLAYING),
                        surfaceState = surfaceState,
                        panels = if (showPanel) listOf(PlayPanel(0, track)) else emptyList(),
                        visualizerOpacity = 0f,
                        visualizerLight = 0f,
                        liveScene = handle,
                    )
                }
            }
        }
        // The fog resolves through a LaunchedEffect that resumes on Dispatchers.Default; the
        // first such resume lands under this harness (see CoverFogBitmapHoldTest), unlike a
        // second one triggered by the same composable, which is why this scene stays on one
        // track and one panel throughout.
        compose.waitUntil(timeoutMillis = 5_000) { handle.fog != null }
        assertTrue("the live panel's scene state must be published too", handle.state != null)

        showPanel = false
        compose.waitForIdle()

        assertNull("the handle clears once the live panel leaves", handle.fog)
        assertNull("the handle clears once the live panel leaves", handle.state)
    }

    @Test
    fun nav_15d_growing_frames_do_not_reset_the_scene() {
        val handle = LiveSceneHandle()
        val analysis = GrowingSpectrogramAnalysis(frameCount = 4)
        showGrowingScene(handle, analysis)
        // Deterministic despite the wait: the waits only let composition and the fog's
        // first resume land. The driver never ticks here — the unbound controller is not
        // resumed, so `sceneFramesAllowed` is false — and the only stepping of the state
        // is the test's own, so nothing moves the values compared below in the meantime.
        compose.waitUntil(timeoutMillis = 5_000) { handle.fog != null && handle.state != null }
        val first = checkNotNull(handle.state)
        assertEquals(4, first.frames.frameCount)
        val before = compose.runOnIdle {
            first.advanceOilFilmBy(5f)
            first.advanceTo(3)
            Triple(first.oilFilmSeconds, first.fogBands.copyOf(), first.motionBands.copyOf())
        }

        analysis.grow(frameCount = 12)
        compose.waitUntil(timeoutMillis = 5_000) { handle.state?.frames?.frameCount == 12 }

        assertTrue("the scene state was replaced instead of adopting the frames", handle.state === first)
        compose.runOnIdle {
            assertEquals("the oil film restarted", before.first, first.oilFilmSeconds, 0f)
            assertArrayEquals("the fog envelopes restarted", before.second, first.fogBands, 0f)
            assertArrayEquals("the motion envelopes restarted", before.third, first.motionBands, 0f)
        }
    }

    @Test
    fun nav_15d_the_scene_keeps_the_decoded_frames_until_the_final_ones_arrive() {
        val handle = LiveSceneHandle()
        val analysis = GrowingSpectrogramAnalysis(frameCount = 4)
        showGrowingScene(handle, analysis)
        compose.waitUntil(timeoutMillis = 5_000) { handle.state?.frames?.frameCount == 4 }

        // The decode ended and the final spectrogram is not delivered yet.
        analysis.endWithoutResult()
        compose.waitForIdle()

        assertEquals(
            "the scene fell back to no frames between the partial and the final analysis",
            4,
            handle.state?.frames?.frameCount,
        )
    }

    @Test
    fun nav_15d_only_the_live_panel_asks_for_the_decoded_part() {
        val handle = LiveSceneHandle()
        val analysis = GrowingSpectrogramAnalysis(frameCount = 4)
        val live = sceneEngineTrack(id = 17)
        val neighbour = sceneEngineTrack(id = 18)
        showGrowingScene(handle, analysis, listOf(PlayPanel(0, live), PlayPanel(1, neighbour)))
        compose.waitUntil(timeoutMillis = 5_000) { handle.state?.frames?.frameCount == 4 }
        compose.waitForIdle()

        assertTrue("the neighbour panel was never composed", neighbour.id in analysis.spectrogramTrackIds)
        assertTrue("the live panel never asked", live.id in analysis.polledTrackIds)
        assertFalse("a neighbour panel polled", neighbour.id in analysis.polledTrackIds)
    }

    private fun showGrowingScene(
        handle: LiveSceneHandle,
        analysis: GrowingSpectrogramAnalysis,
        panels: List<PlayPanel> = listOf(PlayPanel(0, sceneEngineTrack())),
    ) {
        val surfaceState = MobileSurfaceViewModel()
        compose.setContent {
            val theme = MobileThemeSelection(
                palette = MobileTheme.NOCTURNE,
                colorScheme = AndroidColorScheme.SYSTEM,
                dynamicAvailable = false,
            )
            RepriseTheme(theme, darkPalette = true) {
                CompositionLocalProvider(
                    LocalAmbientMotionController provides AmbientMotionController(),
                    LocalVisualSceneEngineFactory provides RecordingSceneEngineFactory(),
                    LocalTrackAnalysis provides analysis,
                ) {
                    NowPlayingScene(
                        track = panels.first().track,
                        playback = PlaybackUiState(state = AndroidPlaybackState.PLAYING),
                        surfaceState = surfaceState,
                        panels = panels,
                        visualizerOpacity = 0f,
                        visualizerLight = 0f,
                        liveScene = handle,
                    )
                }
            }
        }
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
                    visualizerLight = 0f,
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
        withNeighbour: Boolean = false,
        currentIndex: Int = 0,
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
                val panels = if (withNeighbour) {
                    listOf(PlayPanel(0, track), PlayPanel(1, sceneEngineTrack(id = 9002)))
                } else {
                    listOf(PlayPanel(0, track))
                }
                NowPlayingScene(
                    track = track,
                    playback = PlaybackUiState(state = AndroidPlaybackState.PLAYING),
                    surfaceState = surfaceState,
                    positionPx = positionPx,
                    currentIndex = currentIndex,
                    panels = panels,
                    visualizerOpacity = 1f,
                    visualizerLight = 1f,
                )
            }
        }
    }
}

/** A track the phone is still decoding: only [loadProgress] answers, and the answer grows. */
private class GrowingSpectrogramAnalysis(frameCount: Int) : TrackAnalysisPort {
    private var frameCount by mutableIntStateOf(frameCount)
    private var decoding = true
    val polledTrackIds = mutableSetOf<Long>()
    val spectrogramTrackIds = mutableSetOf<Long>()
    override var revision by mutableLongStateOf(0L)
        private set

    fun grow(frameCount: Int) {
        this.frameCount = frameCount
        revision += 1L
    }

    fun endWithoutResult() {
        decoding = false
        revision += 1L
    }

    override fun prepare(trackId: Long) = Unit

    override fun loadBars(trackId: Long, count: Int, deliver: (List<SpectralBar>?) -> Unit) =
        deliver(null)

    override fun loadSpectrogram(trackId: Long, deliver: (SpectrogramFrames?) -> Unit) {
        spectrogramTrackIds += trackId
        deliver(null)
    }

    override fun loadProgress(
        trackId: Long,
        count: Int,
        deliver: (PartialTrackAnalysis?) -> Unit,
    ) {
        polledTrackIds += trackId
        deliver(
            PartialTrackAnalysis(
                coveredFraction = frameCount / 20f,
                bars = emptyList(),
                frames = SpectrogramFrames(24, 20, ByteArray(24 * frameCount) { 128.toByte() }),
            ).takeIf { decoding },
        )
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

/**
 * [distinctEngines] hands out a fresh [RecordingSceneEngine] per [create] call instead of the one
 * [engine] every other test above shares, so a test can tell apart the engines the live slot
 * passes through across a swipe (see [createdEngines]).
 */
private class RecordingSceneEngineFactory(
    private val sceneRecord: List<Float> = emptyList(),
    private val distinctEngines: Boolean = false,
) : VisualSceneEngineFactory {
    val engine = RecordingSceneEngine(sceneRecord)
    val createdEngines = mutableListOf<RecordingSceneEngine>()
    var created = 0
        private set

    override fun create(): VisualSceneEngine {
        created += 1
        val instance = if (distinctEngines) RecordingSceneEngine(sceneRecord) else engine
        createdEngines += instance
        return instance
    }
}

/** Records the calls; [sceneRecord] is what `scene()` hands back, empty by default. */
private class RecordingSceneEngine(
    private val sceneRecord: List<Float> = emptyList(),
) : VisualSceneEngine {
    var ticks = 0
        private set
    var sceneCalls = 0
        private set
    var tintedCalls = 0
        private set
    private var reportedBands: FloatArray = FloatArray(0)
    private var reportedAdoptableBands: FloatArray? = null
    val adoptedShapes = mutableListOf<FloatArray>()

    /**
     * Records [noteTrackChanged] and [adoptShape] calls in the order they land, so a test can pin
     * that the real engine's `has_ingested`-clearing reset happens before the adopted shape is
     * installed, not after (which would wipe it straight back out — see
     * `rememberVisualSceneEngine`'s comment on effect declaration order).
     */
    val callSequence = mutableListOf<String>()

    fun setCurrentBands(bands: FloatArray) {
        reportedBands = bands
    }

    /** What [adoptableBands] reports; until set it mirrors the displayed bars, as the default does. */
    fun setAdoptableBands(bands: FloatArray) {
        reportedAdoptableBands = bands
    }

    override fun setAccent(red: Float, green: Float, blue: Float) = Unit
    override fun setPlaying(playing: Boolean) = Unit
    override fun noteTrackChanged() {
        callSequence += "noteTrackChanged"
    }
    override fun ingestBands(bands: FloatArray) = Unit
    override fun currentBands(): FloatArray = reportedBands
    override fun adoptableBands(): FloatArray = reportedAdoptableBands ?: reportedBands
    override fun adoptShape(bands: FloatArray) {
        callSequence += "adoptShape"
        adoptedShapes += bands
    }
    override fun tick() {
        ticks += 1
    }
    override fun scene(width: Float, height: Float): List<Float> {
        sceneCalls += 1
        return sceneRecord
    }
    override fun sceneBytesTinted(
        width: Float,
        height: Float,
        red: Float,
        green: Float,
        blue: Float,
    ): ByteArray {
        tintedCalls += 1
        return ByteArray(0)
    }
    override fun close() = Unit
}

private fun sceneEngineTrack(id: Long = 17) = LibraryTrack(
    id = id,
    uri = "content://provider/song-$id.flac",
    title = "Song",
    artist = "Artist",
    album = "Album",
    durationMs = 180_000,
    playCount = 0,
    rating = 0,
)

private const val DISPLAY_FRAME_MS = 16L

/** One well-formed rectangle record, `[kind, r, g, b, a, width, glow, pointCount, x, y, w, h]`. */
private val FLAT_RECT_RECORD = listOf(0f, 1f, 1f, 1f, 1f, 0f, 0f, 4f, 0f, 0f, 10f, 10f)

private val SEED_BANDS = floatArrayOf(0.2f, 0.5f, 0.8f)
private val DECAYED_BANDS = floatArrayOf(0.02f, 0.05f, 0.04f)
