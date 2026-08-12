package de.reprise.spike

import androidx.activity.ComponentActivity
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.click
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performTouchInput
import de.reprise.spike.ui.theme.RepriseTheme
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
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
    fun coverDragBelowThresholdSpringsBackWithoutChangingTrack() {
        val controls = GestureRecordingControls()
        compose.setContent { testNowPlayingSheet(controls = controls) }

        compose.onNodeWithTag("now-playing-gestures").performTouchInput {
            down(Offset(width * 0.65f, height * 0.3f))
            moveTo(Offset(width * 0.55f, height * 0.3f))
            up()
        }

        assertEquals(0, controls.nextCalls)
        assertEquals(0, controls.previousCalls)
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
        assertEquals(0, engines.created)
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
        assertEquals(0, engines.created)
    }

    @Composable
    private fun testNowPlayingSheet(
        controls: PlaybackControls = DisconnectedPlaybackControls,
        preference: VisualizerPreference = DisconnectedVisualizerPreference,
        engines: VisualSceneEngineFactory = RecordingVisualEngineFactory(),
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
            ) {
                NowPlayingSheet(
                    track = gestureTrack(),
                    playback = gesturePlayback(),
                    close = close,
                )
            }
        }
    }
}

private class RecordingVisualizerPreference(
    private var stored: AndroidStoredVisualizer = AndroidStoredVisualizer.Cover,
) : VisualizerPreference {
    var reads = 0
        private set
    val writes = mutableListOf<AndroidVisualizerChoice>()

    override fun visualizerSetting(): AndroidStoredVisualizer {
        reads += 1
        return stored
    }

    override fun setVisualizer(choice: AndroidVisualizerChoice) {
        writes += choice
        stored = when (choice) {
            AndroidVisualizerChoice.COVER -> AndroidStoredVisualizer.Cover
            AndroidVisualizerChoice.SPECTRUM -> AndroidStoredVisualizer.Spectrum
            AndroidVisualizerChoice.PREVIEW_BAND -> AndroidStoredVisualizer.PreviewBand
            AndroidVisualizerChoice.AMBIENT -> AndroidStoredVisualizer.Ambient
        }
    }
}

private class RecordingVisualEngineFactory : VisualSceneEngineFactory {
    var created = 0
        private set

    override fun create(): VisualSceneEngine {
        created += 1
        return object : VisualSceneEngine {
            override fun setAccent(red: Float, green: Float, blue: Float) = Unit
            override fun setPlaying(playing: Boolean) = Unit
            override fun noteTrackChanged() = Unit
            override fun ingestBands(bands: FloatArray) = Unit
            override fun tick() = Unit
            override fun scene(width: Float, height: Float): List<Float> = emptyList()
            override fun close() = Unit
        }
    }
}

private class GestureRecordingControls : PlaybackControls by DisconnectedPlaybackControls {
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

private fun gestureTrack() = LibraryTrack(
    id = 830,
    uri = "content://provider/document/song.flac",
    title = "Song",
    artist = "Artist",
    album = "Album",
    durationMs = 100_000,
    playCount = 27,
    rating = 2,
)
