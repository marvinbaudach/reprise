package de.reprise.spike

import android.os.Looper
import androidx.compose.ui.test.assertHasClickAction
import androidx.compose.ui.test.assertWidthIsEqualTo
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.unit.dp
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import uniffi.reprise_android_ffi.AndroidPlaybackSnapshot
import uniffi.reprise_android_ffi.AndroidPlaybackState
import uniffi.reprise_android_ffi.AndroidRepeatMode
import uniffi.reprise_android_ffi.AndroidTrackSpectrogram

@RunWith(RobolectricTestRunner::class)
@Config(
    sdk = [36],
    qualifiers = "w412dp-h916dp-port",
    application = ConfigurationTestApplication::class,
)
class MainActivityVisualizerTest {
    @get:Rule
    val compose = createAndroidComposeRule<MainActivity>()

    private val application: ConfigurationTestApplication
        get() = RuntimeEnvironment.getApplication() as ConfigurationTestApplication

    @Before
    fun startFromThePlayedView() {
        application.getSharedPreferences("reprise_android", 0)
            .edit()
            .remove(INJECTED_NOW_PLAYING_VIEW_KEY)
            .commit()
    }

    @After
    fun releaseTheService() {
        application.releaseService()
    }

    @Test
    fun scene_toggle_persistence_idle_controls_and_transition_hit_testing() {
        openNowPlaying()
        compose.onNodeWithTag("now-playing-player").assertExists()
        compose.onNodeWithTag("now-playing-scene-cover").assertWidthIsEqualTo(272.dp)
        val pause = compose.onNodeWithTag("now-playing-play")
        pause.assertHasClickAction().performClick()

        compose.mainClock.autoAdvance = false
        compose.onNodeWithTag("now-playing-enter-fullscreen").performClick()
        assertEquals("visualizer", storedView())
        compose.mainClock.advanceTimeBy(160)
        compose.onNodeWithTag("now-playing-visualizer").assertExists()
        pause.assertHasClickAction().performClick()
        compose.mainClock.advanceTimeBy(160)
        pause.assertHasClickAction().performClick()

        compose.mainClock.advanceTimeBy(4_301)
        compose.onNodeWithTag("now-playing-controls-faded").assertExists()
        compose.onNodeWithTag("now-playing-scene").performClick()
        compose.mainClock.advanceTimeBy(16)
        compose.onNodeWithTag("now-playing-controls-visible").assertExists()
        compose.mainClock.advanceTimeBy(300)

        compose.onNodeWithContentDescription("Return to player").performClick()
        assertEquals("player", storedView())
        compose.mainClock.advanceTimeBy(320)
        compose.onNodeWithTag("now-playing-player").assertExists()
        compose.onNodeWithTag("now-playing-enter-fullscreen").performClick()
        assertEquals("visualizer", storedView())
        compose.mainClock.advanceTimeBy(320)

        compose.mainClock.autoAdvance = true
        compose.activityRule.scenario.recreate()
        shadowOf(Looper.getMainLooper()).idle()
        application.service.republish()
        shadowOf(Looper.getMainLooper()).idle()
        compose.waitForIdle()
        if (compose.onAllNodesWithTag("library-mini-player").fetchSemanticsNodes().isNotEmpty()) {
            compose.onNodeWithTag("library-mini-player").performClick()
        }
        compose.onNodeWithTag("now-playing-visualizer").assertExists()
    }

    @Test
    fun spectrogram_read_uses_the_analysis_lane_and_treats_missing_data_as_ordinary() {
        val delivered = mutableListOf<de.reprise.spike.scene.SpectrogramFrames?>()
        val loader = TrackAnalysisLoader(
            importAnalysis = {},
            readBars = { _, _ -> null },
            readSpectrogram = { trackId ->
                if (trackId == 7L) {
                    AndroidTrackSpectrogram(2u, 20u, byteArrayOf(1, 2, 3, 4))
                } else {
                    null
                }
            },
            onMainThread = { work -> work() },
        )

        loader.loadSpectrogram(7) { delivered.add(it) }
        loader.loadSpectrogram(8) { delivered.add(it) }
        loader.shutdownForTest()
        assertEquals(2, delivered.single { it != null }?.bandCount)
        assertEquals(20, delivered.single { it != null }?.frameRateHz)
        assertEquals(4, delivered.single { it != null }?.band(1, 1))
        assertEquals(1, delivered.count { it == null })
    }

    private fun openNowPlaying() {
        application.animationsEnabled = false
        compose.activityRule.scenario.recreate()
        shadowOf(Looper.getMainLooper()).idle()
        application.service.publish(m9bSnapshot(trackId = 1))
        shadowOf(Looper.getMainLooper()).idle()
        compose.waitForIdle()
        if (compose.onAllNodesWithTag("library-mini-player").fetchSemanticsNodes().isNotEmpty()) {
            compose.onNodeWithTag("library-mini-player").performClick()
        }
    }

    private fun storedView(): String? = application
        .getSharedPreferences("reprise_android", 0)
        .getString(INJECTED_NOW_PLAYING_VIEW_KEY, null)
}

internal fun m9bSnapshot(trackId: Long) = AndroidPlaybackSnapshot(
    state = AndroidPlaybackState.PLAYING,
    currentIndex = 0u,
    currentTrackId = trackId,
    currentTrackUri = "content://provider/document/$trackId.flac",
    positionMs = 12_000,
    durationMs = 120_000,
    automaticAdvanceCount = 0u,
    shuffled = false,
    repeat = AndroidRepeatMode.OFF,
    error = null,
)
