package io.github.marvinbaudach.reprise

import androidx.activity.ComponentActivity
import androidx.compose.foundation.interaction.DragInteraction
import androidx.compose.foundation.interaction.Interaction
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.semantics.ProgressBarRangeInfo
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.SemanticsNodeInteraction
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.click
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performTouchInput
import io.github.marvinbaudach.reprise.ui.theme.RepriseTheme
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.runBlocking
import uniffi.reprise_android_ffi.AndroidColorScheme
import uniffi.reprise_android_ffi.AndroidPlaybackState
import uniffi.reprise_android_ffi.AndroidStoredVisualizer
import uniffi.reprise_android_ffi.AndroidVisualizerChoice

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36], qualifiers = "w500dp-h1000dp")
class NowPlayingGesturesTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    fun aCancelledSeekGestureReturnsTheHeadToThePlaybackPosition() {
        val playback = mutableStateOf(gesturePlayback())
        val surfaceState = MobileSurfaceViewModel()
        val interactions = RecordingSeekInteractionSource()
        compose.setContent {
            TestSeekSlider(playback.value, surfaceState, interactions)
        }
        val slider = compose.onNodeWithTag("now-playing-seek")
        slider.performTouchInput {
            down(Offset(width * 0.2f, centerY))
            moveTo(Offset(width * 0.6f, centerY))
        }
        compose.waitForIdle()
        assertTrue(slider.progress() > 50_000f)

        compose.runOnUiThread {
            interactions.cancelDrag()
            playback.value = playback.value.copy(positionMs = 30_000)
        }
        compose.waitForIdle()

        assertEquals(30_000f, slider.progress(), 0.5f)
    }

    @Test
    fun aStillFingerKeepsTheHead() {
        val playback = mutableStateOf(gesturePlayback())
        val surfaceState = MobileSurfaceViewModel()
        val interactions = RecordingSeekInteractionSource()
        compose.setContent {
            TestSeekSlider(playback.value, surfaceState, interactions)
        }
        val slider = compose.onNodeWithTag("now-playing-seek")
        slider.performTouchInput {
            down(Offset(width * 0.2f, centerY))
            moveTo(Offset(width * 0.6f, centerY))
        }
        compose.waitForIdle()
        val draggedPosition = slider.progress()

        compose.runOnUiThread {
            playback.value = playback.value.copy(positionMs = 30_000)
        }
        compose.waitForIdle()

        assertEquals(draggedPosition, slider.progress(), 0.5f)
    }

    @Test
    fun aCancelFromTheOutgoingTrackDoesNotMoveTheNewTracksHead() {
        val surfaceState = MobileSurfaceViewModel()
        val outgoingTrackId = 830L
        val newTrackId = 831L
        surfaceState.dragTo(outgoingTrackId, positionMs = 60_000)

        surfaceState.releaseScrub(newTrackId)

        val newHead = surfaceState.seekPosition(newTrackId, fallbackPositionMs = 40_000)
        assertEquals(40_000L, newHead.positionMs)
        val outgoingHead = surfaceState.seekPosition(outgoingTrackId, fallbackPositionMs = 0)
        assertEquals(60_000L, outgoingHead.positionMs)
        assertTrue(outgoingHead.isDragging)
    }

    @Test
    fun coverDragPastThresholdSkipsToTheNextTrack() {
        val controls = GestureRecordingControls()
        compose.setContent { testNowPlayingSheet(controls = controls) }

        compose.onNodeWithTag("now-playing-gestures").performTouchInput {
            down(Offset(width * 0.75f, height * 0.3f))
            moveTo(Offset(width * 0.35f, height * 0.3f))
            up()
        }

        assertEquals(1, controls.nextCalls)
    }

    @Test
    fun a_track_change_keeps_the_populated_panel_window_until_its_reload_answers() {
        val controls = DelayedPanelWindowControls()
        val track = mutableStateOf(gestureTrack())
        val currentIndex = mutableIntStateOf(1)
        compose.setContent {
            val window = rememberPlayPanelWindow(track.value, currentIndex.intValue, controls)
            Text(
                window.panels.joinToString(",") { panel ->
                    "${panel.index}:${panel.track?.id ?: "pending"}"
                },
            )
        }
        compose.waitForIdle()
        compose.onNodeWithText("0:829,1:830,2:831").assertIsDisplayed()

        compose.runOnUiThread {
            track.value = gestureTrack(id = 831, title = "Next song")
            currentIndex.intValue = 2
        }
        compose.waitForIdle()

        compose.onNodeWithText("1:830,2:831").assertIsDisplayed()
    }

    @Test
    fun an_index_that_moves_before_its_track_answers_does_not_restamp_the_window() {
        val controls = DelayedPanelWindowControls()
        val track = mutableStateOf(gestureTrack())
        val currentIndex = mutableIntStateOf(1)
        val trackIsStale = mutableStateOf(false)
        compose.setContent {
            val window = rememberPlayPanelWindow(
                track.value,
                currentIndex.intValue,
                controls,
                trackIsStale.value,
            )
            Text(
                window.panels.joinToString(",") { panel ->
                    "${panel.index}:${panel.track?.id ?: "pending"}"
                },
            )
        }
        compose.waitForIdle()
        compose.onNodeWithText("0:829,1:830,2:831").assertIsDisplayed()

        // The player has moved on; the metadata query has not answered yet, so
        // the track still describes the song the swipe is leaving behind.
        compose.runOnUiThread {
            currentIndex.intValue = 2
            trackIsStale.value = true
        }
        compose.waitForIdle()

        // 830 is the outgoing track. Stamping it at index 2 is what used to
        // carry the old cover into the centre; the prefetched 831 stays there.
        compose.onNodeWithText("1:830,2:831").assertIsDisplayed()

        // The answer lands: the pair agrees again and the window catches up.
        compose.runOnUiThread {
            track.value = gestureTrack(id = 831, title = "Next song")
            trackIsStale.value = false
        }
        compose.waitForIdle()

        compose.onNodeWithText("1:830,2:831").assertIsDisplayed()
    }

    @Test
    fun two_stale_external_advances_keep_an_unstamped_panel_at_the_live_index() {
        val controls = DelayedPanelWindowControls()
        val currentIndex = mutableIntStateOf(1)
        val trackIsStale = mutableStateOf(false)
        compose.setContent {
            val window = rememberPlayPanelWindow(
                gestureTrack(),
                currentIndex.intValue,
                controls,
                trackIsStale.value,
            )
            Text(
                "${window.firstIndex}..${window.lastIndex}|" +
                    window.panels.joinToString(",") { panel ->
                        "${panel.index}:${panel.track?.id ?: "pending"}"
                    },
            )
        }
        compose.waitForIdle()

        compose.runOnUiThread {
            currentIndex.intValue = 2
            trackIsStale.value = true
        }
        compose.waitForIdle()
        compose.onNodeWithText("1..2|1:830,2:831").assertIsDisplayed()

        compose.runOnUiThread { currentIndex.intValue = 3 }
        compose.waitForIdle()
        compose.onNodeWithText("2..3|2:831,3:pending").assertIsDisplayed()
        assertEquals(1, controls.requestCount)
    }

    @Test
    fun a_drag_can_settle_to_the_retained_neighbour_after_two_stale_advances() {
        val window = windowAfterStaleAdvances(2, 3)
        val gesture = PlayGestureState(
            width = 400f,
            height = 800f,
            animationsEnabled = true,
            currentIndex = 3,
            firstIndex = window.firstIndex,
            lastIndex = window.lastIndex,
        ).apply {
            begin(horizontalAllowed = true, verticalAllowed = true)
            dragBy(deltaX = 89f, deltaY = 0f)
        }

        assertEquals(PlayGestureDecision.PREVIOUS, gesture.settle(0f, 0f))
    }

    @Test
    fun three_stale_advances_fill_the_gap_with_a_content_free_panel() {
        val window = windowAfterStaleAdvances(2, 3, 4)

        assertEquals(listOf(2, 3, 4), window.panels.map(PlayPanel::index))
        assertEquals(listOf(831L, null, null), window.panels.map { panel -> panel.track?.id })
    }

    @Test
    fun an_initially_stale_track_starts_with_an_unstamped_centre_panel() {
        val controls = DelayedPanelWindowControls()
        compose.setContent {
            val window = rememberPlayPanelWindow(
                gestureTrack(),
                currentIndex = 5,
                controls,
                trackIsStale = true,
            )
            Text("${window.panels.single().index}:${window.panels.single().track?.id ?: "pending"}")
        }
        compose.waitForIdle()

        compose.onNodeWithText("5:pending").assertIsDisplayed()
        assertEquals(0, controls.requestCount)
    }

    @Test
    fun a_callback_abandoned_by_a_stale_pass_cannot_overwrite_the_window() {
        val controls = HoldingPanelWindowControls()
        val trackIsStale = mutableStateOf(false)
        compose.setContent {
            val window = rememberPlayPanelWindow(
                gestureTrack(),
                currentIndex = 1,
                controls,
                trackIsStale.value,
            )
            Text(window.panels.joinToString(",") { panel -> "${panel.index}:${panel.track?.id}" })
        }
        compose.waitForIdle()
        assertEquals(1, controls.requestCount)

        compose.runOnUiThread { trackIsStale.value = true }
        compose.waitForIdle()
        assertEquals(1, controls.requestCount)

        compose.runOnUiThread { controls.releaseFirst() }
        compose.waitForIdle()
        compose.onNodeWithText("1:830").assertIsDisplayed()
    }

    @Test
    fun externalAdvanceMidDragReanchorsBeforeTheFingerCommitsForward() {
        val controls = GestureRecordingControls(
            upcomingRows = (828L..833L).map { id -> gestureTrack(id, "Song $id") },
        )
        val track = mutableStateOf(gestureTrack())
        val playback = mutableStateOf(gesturePlayback().copy(currentIndex = 4))
        compose.setContent {
            testNowPlayingSheet(
                controls = controls,
                track = track.value,
                playback = playback.value,
            )
        }
        val gestures = compose.onNodeWithTag("now-playing-gestures")
        gestures.performTouchInput {
            down(Offset(width * 0.5f, height * 0.3f))
            moveTo(Offset(width * 0.9f, height * 0.3f))
        }

        compose.runOnUiThread {
            track.value = gestureTrack(831, "Next song")
            playback.value = playback.value.copy(currentIndex = 5, currentTrackId = 831)
        }
        compose.waitForIdle()
        gestures.performTouchInput {
            moveTo(Offset(width * 0.6f, height * 0.3f))
            up()
        }

        assertEquals(1, controls.nextCalls)
        assertEquals(0, controls.previousCalls)
    }

    @Test
    fun coverDragBelowThresholdSpringsBackWithoutChangingTrack() {
        val controls = GestureRecordingControls()
        compose.setContent { testNowPlayingSheet(controls = controls) }

        compose.onNodeWithTag("now-playing-gestures").performTouchInput {
            down(Offset(width * 0.65f, height * 0.3f))
            moveTo(Offset(width * 0.60f, height * 0.3f))
            advanceEventTime(250)
            up()
        }

        assertEquals(0, controls.nextCalls)
        assertEquals(0, controls.previousCalls)
    }

    @Test
    fun reducedMotionCommitSnapsToItsTargetWithoutAnimationOrConfirmationCue() = runBlocking {
        val motionPasses = mutableListOf<String>()
        settleNowPlayingPosition(
            target = 500f,
            animationsEnabled = false,
            animate = { motionPasses += "animate" },
            snap = { target -> motionPasses += "snap:$target" },
        )
        val cueGate = TrackChangeCueGate()
        cueGate.observe(trackId = 830, animationsEnabled = false)

        assertEquals(listOf("snap:500.0"), motionPasses)
        assertFalse(cueGate.observe(trackId = 831, animationsEnabled = false))
    }

    @Test
    fun downwardDragClosesTheSheet() {
        var closed = false
        compose.setContent { testNowPlayingSheet(close = { closed = true }) }

        compose.onNodeWithTag("now-playing-gestures").performTouchInput {
            down(Offset(centerX, height * 0.3f))
            moveTo(Offset(centerX, height * 0.55f))
            up()
        }

        assertTrue(closed)
    }

    @Test
    fun doubleTapOnTheLeftSeeksBackTenSecondsAndShowsItsMarker() {
        val controls = GestureRecordingControls()
        val preference = RecordingVisualizerPreference()
        compose.setContent {
            testNowPlayingSheet(controls = controls, preference = preference)
        }

        compose.onNodeWithTag("now-playing-gestures").performTouchInput {
            val point = Offset(width * 0.25f, height * 0.3f)
            down(point)
            up()
            advanceEventTime(100)
            down(point)
            up()
        }

        assertEquals(listOf(10_000L), controls.seekPositions)
        compose.onNodeWithText("−10 s").assertIsDisplayed()
        compose.mainClock.advanceTimeBy(350)
        assertTrue(preference.writes.isEmpty())
    }

    @Test
    fun visualizerCrossfadeUsesTheAcceptedDuration() {
        assertEquals(220, VISUALIZER_CROSSFADE_MS)
    }

    @Test
    fun singleTapOnTheCoverSwitchesToTheSpectrumAndBack() {
        val preference = RecordingVisualizerPreference()
        val engines = RecordingVisualEngineFactory()
        compose.setContent {
            testNowPlayingSheet(preference = preference, engines = engines)
        }

        compose.onNodeWithTag("now-playing-gestures").performTouchInput {
            click(Offset(width * 0.5f, height * 0.34f))
        }
        compose.mainClock.advanceTimeBy(350)
        compose.waitForIdle()

        assertEquals(listOf(AndroidVisualizerChoice.SPECTRUM), preference.writes)
        assertEquals(1, engines.created)

        compose.onNodeWithTag("now-playing-gestures").performTouchInput {
            click(Offset(width * 0.5f, height * 0.34f))
        }
        compose.mainClock.advanceTimeBy(350)
        compose.waitForIdle()

        assertEquals(
            listOf(AndroidVisualizerChoice.SPECTRUM, AndroidVisualizerChoice.COVER),
            preference.writes,
        )
    }

    @Test
    fun aSecondTapBeforeTheAnswerSubmitsTheOppositeTargetWithoutMovingEarly() {
        val preference = RecordingVisualizerPreference(answerImmediately = false)
        val engines = RecordingVisualEngineFactory()
        compose.setContent {
            testNowPlayingSheet(preference = preference, engines = engines)
        }
        val gestures = compose.onNodeWithTag("now-playing-gestures")

        gestures.performTouchInput { click(Offset(width * 0.5f, height * 0.34f)) }
        compose.mainClock.advanceTimeBy(350)
        gestures.performTouchInput { click(Offset(width * 0.5f, height * 0.34f)) }
        compose.mainClock.advanceTimeBy(350)
        compose.waitForIdle()

        assertEquals(
            listOf(AndroidVisualizerChoice.SPECTRUM, AndroidVisualizerChoice.COVER),
            preference.writes,
        )
        assertEquals("the spectrum must wait for its answer", 0, engines.sceneCalls)

        preference.answerNextSuccess()
        preference.answerNextSuccess()
    }

    @Test
    fun singleTapOutsideTheCoverDoesNotSwitch() {
        val preference = RecordingVisualizerPreference()
        val engines = RecordingVisualEngineFactory()
        compose.setContent {
            testNowPlayingSheet(preference = preference, engines = engines)
        }

        compose.onNodeWithTag("now-playing-gestures").performTouchInput {
            click(Offset(width * 0.5f, height * 0.08f))
        }
        compose.mainClock.advanceTimeBy(350)
        compose.waitForIdle()

        assertTrue(preference.writes.isEmpty())
        assertEquals(1, engines.created)
        assertEquals(0, engines.sceneCalls)
    }

    @Test
    fun persistedSpectrumIsRestoredWhenThePlayViewIsReentered() {
        val preference = RecordingVisualizerPreference(AndroidStoredVisualizer.Spectrum)
        val engines = RecordingVisualEngineFactory()
        val incarnation = mutableIntStateOf(0)
        compose.setContent {
            key(incarnation.intValue) {
                testNowPlayingSheet(preference = preference, engines = engines)
            }
        }
        compose.waitForIdle()

        assertEquals(1, preference.reads)
        assertEquals(1, engines.created)

        compose.runOnUiThread {
            incarnation.intValue += 1
        }
        compose.waitForIdle()

        assertEquals(2, preference.reads)
        assertEquals(2, engines.created)
        assertTrue(preference.writes.isEmpty())
    }

    @Test
    fun unsupportedStoredChoiceFallsBackToTheCoverWithoutRewritingIt() {
        val preference = RecordingVisualizerPreference(
            AndroidStoredVisualizer.Unsupported("future-mode"),
        )
        val engines = RecordingVisualEngineFactory()
        compose.setContent {
            testNowPlayingSheet(preference = preference, engines = engines)
        }
        compose.waitForIdle()

        assertEquals(1, preference.reads)
        assertTrue(preference.writes.isEmpty())
        assertEquals(1, engines.created)
        assertEquals(0, engines.sceneCalls)
    }

    @Composable
    private fun testNowPlayingSheet(
        controls: PlaybackControls = DisconnectedPlaybackControls,
        preference: VisualizerPreference = DisconnectedVisualizerPreference,
        engines: VisualSceneEngineFactory = RecordingVisualEngineFactory(),
        controller: AmbientMotionController = AmbientMotionController(),
        track: LibraryTrack = gestureTrack(),
        playback: PlaybackUiState = gesturePlayback(),
        close: () -> Unit = {},
    ) {
        val theme = MobileThemeSelection(
            palette = MobileTheme.NOCTURNE,
            colorScheme = AndroidColorScheme.SYSTEM,
            dynamicAvailable = false,
        )
        RepriseTheme(theme, darkPalette = true) {
            CompositionLocalProvider(
                LocalPlaybackControls provides controls,
                LocalVisualizerPreference provides preference,
                LocalVisualSceneEngineFactory provides engines,
                LocalAmbientMotionController provides controller,
            ) {
                NowPlayingSheet(
                    track = track,
                    playback = playback,
                    close = close,
                )
            }
        }
    }

    @Composable
    private fun TestSeekSlider(
        playback: PlaybackUiState,
        surfaceState: MobileSurfaceViewModel,
        interactionSource: MutableInteractionSource,
    ) {
        val theme = MobileThemeSelection(
            palette = MobileTheme.NOCTURNE,
            colorScheme = AndroidColorScheme.SYSTEM,
            dynamicAvailable = false,
        )
        RepriseTheme(theme, darkPalette = true) {
            CompositionLocalProvider(LocalPlaybackControls provides DisconnectedPlaybackControls) {
                SpectralSeekSlider(
                    trackId = gestureTrack().id,
                    playback = playback,
                    surfaceState = surfaceState,
                    interactionSource = interactionSource,
                )
            }
        }
    }

    private fun SemanticsNodeInteraction.progress(): Float =
        fetchSemanticsNode().config
            .getOrNull(SemanticsProperties.ProgressBarRangeInfo)
            ?.current
            ?: error("No progress semantics")
}

private class RecordingSeekInteractionSource : MutableInteractionSource {
    private val delegate = MutableInteractionSource()
    private var dragStart: DragInteraction.Start? = null

    override val interactions: Flow<Interaction>
        get() = delegate.interactions

    override suspend fun emit(interaction: Interaction) {
        remember(interaction)
        delegate.emit(interaction)
    }

    override fun tryEmit(interaction: Interaction): Boolean {
        remember(interaction)
        return delegate.tryEmit(interaction)
    }

    fun cancelDrag() {
        val start = checkNotNull(dragStart) { "the slider did not begin a drag" }
        check(tryEmit(DragInteraction.Cancel(start))) { "the slider did not accept drag cancel" }
    }

    private fun remember(interaction: Interaction) {
        if (interaction is DragInteraction.Start) dragStart = interaction
    }
}

private class RecordingVisualizerPreference(
    private var stored: AndroidStoredVisualizer = AndroidStoredVisualizer.Cover,
    private val answerImmediately: Boolean = true,
) : VisualizerPreference {
    var reads = 0
        private set
    val writes = mutableListOf<AndroidVisualizerChoice>()
    private val pending = ArrayDeque<Pair<AndroidVisualizerChoice, (Result<Unit>) -> Unit>>()

    override fun visualizerSetting(): AndroidStoredVisualizer {
        reads += 1
        return stored
    }

    override fun setVisualizer(
        choice: AndroidVisualizerChoice,
        report: (Result<Unit>) -> Unit,
    ) {
        writes += choice
        if (!answerImmediately) {
            pending += choice to report
            return
        }
        accept(choice)
        report(Result.success(Unit))
    }

    fun answerNextSuccess() {
        val (choice, report) = pending.removeFirst()
        accept(choice)
        report(Result.success(Unit))
    }

    private fun accept(choice: AndroidVisualizerChoice) {
        stored = when (choice) {
            AndroidVisualizerChoice.COVER -> AndroidStoredVisualizer.Cover
            AndroidVisualizerChoice.SPECTRUM -> AndroidStoredVisualizer.Spectrum
            AndroidVisualizerChoice.PREVIEW_BAND -> AndroidStoredVisualizer.PreviewBand
            AndroidVisualizerChoice.AMBIENT -> AndroidStoredVisualizer.Ambient
        }
    }
}

private fun windowAfterStaleAdvances(vararg indices: Int): PlayPanelWindow =
    indices.fold(
        playPanelWindow(
            currentIndex = 1,
            currentTrackId = 830,
            rows = (829L..831L).map { id -> gestureTrack(id, "Song $id") },
        ),
    ) { window, index ->
        window.advancedTo(gestureTrack(), index, trackIsStale = true)
    }

private class RecordingVisualEngineFactory : VisualSceneEngineFactory {
    var created = 0
        private set
    var sceneCalls = 0
        private set

    override fun create(): VisualSceneEngine {
        created += 1
        return object : VisualSceneEngine {
            override fun setAccent(red: Float, green: Float, blue: Float) = Unit
            override fun setPlaying(playing: Boolean) = Unit
            override fun noteTrackChanged() = Unit
            override fun ingestBands(bands: FloatArray) = Unit
            override fun tick() = Unit
            override fun scene(width: Float, height: Float): List<Float> {
                sceneCalls += 1
                return emptyList()
            }
            override fun close() = Unit
        }
    }
}

private class DelayedPanelWindowControls : PlaybackControls by DisconnectedPlaybackControls {
    var requestCount = 0
        private set

    override fun loadUpcomingTracks(
        window: LibraryWindowRange,
        report: (Result<LibraryWindow<LibraryTrack>>) -> Unit,
    ) {
        requestCount += 1
        if (requestCount != 1) return
        report(
            Result.success(
                LibraryWindow(
                    rows = (829L..833L).map { id -> gestureTrack(id, "Song $id") },
                    total = 5,
                    hasMore = false,
                ),
            ),
        )
    }
}

private class HoldingPanelWindowControls : PlaybackControls by DisconnectedPlaybackControls {
    var requestCount = 0
        private set
    private val heldReports = mutableListOf<(Result<LibraryWindow<LibraryTrack>>) -> Unit>()

    override fun loadUpcomingTracks(
        window: LibraryWindowRange,
        report: (Result<LibraryWindow<LibraryTrack>>) -> Unit,
    ) {
        requestCount += 1
        heldReports += report
    }

    fun releaseFirst() {
        heldReports.removeFirst().invoke(
            Result.success(
                LibraryWindow(
                    rows = (829L..831L).map { id -> gestureTrack(id, "Song $id") },
                    total = 3,
                    hasMore = false,
                ),
            ),
        )
    }
}

private class GestureRecordingControls(
    private val upcomingRows: List<LibraryTrack> =
        listOf(gestureTrack(), gestureTrack(831, "Next song")),
) : PlaybackControls by DisconnectedPlaybackControls {
    val seekPositions = mutableListOf<Long>()
    var nextCalls = 0
        private set
    var previousCalls = 0
        private set

    override fun next() {
        nextCalls += 1
    }

    override fun previous() {
        previousCalls += 1
    }

    override fun seekTo(positionMs: Long) {
        seekPositions += positionMs
    }

    override fun loadUpcomingTracks(
        window: LibraryWindowRange,
        report: (Result<LibraryWindow<LibraryTrack>>) -> Unit,
    ) {
        report(
            Result.success(
                LibraryWindow(
                    rows = upcomingRows,
                    total = upcomingRows.size.toLong(),
                    hasMore = false,
                ),
            ),
        )
    }
}

private fun gesturePlayback() = PlaybackUiState(
    ready = true,
    state = AndroidPlaybackState.PAUSED,
    currentIndex = 0,
    currentTrackId = 830,
    currentTrackUri = "content://provider/document/song.flac",
    positionMs = 20_000,
    durationMs = 100_000,
)

private fun gestureTrack(id: Long = 830, title: String = "Song") = LibraryTrack(
    id = id,
    uri = "content://provider/document/$id.flac",
    title = title,
    artist = "Artist",
    album = "Album",
    durationMs = 100_000,
    playCount = 27,
    rating = 2,
)
