package de.reprise.spike

import android.os.Looper
import androidx.compose.ui.test.assertHasClickAction
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertWidthIsEqualTo
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.width
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
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

/** One frame of the paused test clock. */
private const val FRAME_MS = 16L

/** Half of the scene transition: far enough in that neither end state is showing. */
private const val HALF_TRANSITION_MS = 160L

/** The whole scene transition plus a frame, so it has certainly settled. */
private const val TRANSITION_SETTLED_MS = 336L

/** How long the fullscreen controls stay up without a touch. */
private const val IDLE_MS = 4_000L

/** Idle delay plus the fade, past which the controls are invisible. */
private const val FADED_MS = 4_301L

private val PLAYER_PAUSE_SIZE = 80.dp
private val FULLSCREEN_PAUSE_SIZE = 62.dp

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
    fun the_played_view_opens_with_the_scene_cover_at_its_designed_size() {
        openNowPlaying()
        compose.onNodeWithTag("now-playing-player").assertExists()
        compose.onNodeWithTag("now-playing-scene-cover").assertWidthIsEqualTo(272.dp)
    }

    @Test
    fun entering_the_visualizer_switches_the_scene_and_remembers_the_choice() {
        openNowPlaying()
        enterFullscreen()
        assertEquals("visualizer", storedView())
        compose.onNodeWithTag("now-playing-visualizer").assertExists()
    }

    @Test
    fun leaving_the_visualizer_returns_to_the_player_and_remembers_the_choice() {
        openNowPlaying()
        enterFullscreen()
        compose.onNodeWithContentDescription("Return to player").performClick()
        assertEquals("player", storedView())
        compose.mainClock.advanceTimeBy(TRANSITION_SETTLED_MS)
        compose.onNodeWithTag("now-playing-player").assertExists()
    }

    @Test
    fun the_remembered_visualizer_comes_back_after_a_restart() {
        openNowPlaying()
        enterFullscreen()

        compose.mainClock.autoAdvance = true
        compose.activityRule.scenario.recreate()
        shadowOf(Looper.getMainLooper()).idle()
        application.service.republish()
        shadowOf(Looper.getMainLooper()).idle()
        compose.waitForIdle()
        openTheSheetIfTheLibraryIsShowing()

        compose.onNodeWithTag("now-playing-visualizer").assertExists()
    }

    @Test
    fun the_fullscreen_controls_fade_once_the_idle_delay_has_passed() {
        openNowPlaying()
        enterFullscreen()
        compose.onNodeWithTag("now-playing-controls-visible").assertExists()

        compose.mainClock.advanceTimeBy(FADED_MS)

        compose.onNodeWithTag("now-playing-controls-faded").assertExists()
    }

    @Test
    fun a_touch_on_the_faded_scene_brings_the_controls_back() {
        openNowPlaying()
        enterFullscreen()
        compose.mainClock.advanceTimeBy(FADED_MS)
        compose.onNodeWithTag("now-playing-controls-faded").assertExists()

        compose.onNodeWithTag("now-playing-scene").performTouchInput {
            down(center)
            up()
        }
        compose.mainClock.advanceTimeBy(FRAME_MS)

        compose.onNodeWithTag("now-playing-controls-visible").assertExists()
    }

    /**
     * The faded buttons stay mounted and keep their place on screen, so the spot a
     * user is likeliest to tap — where the pause button was — must wake the controls
     * like every other spot does. A semantics click would not see this: it invokes the
     * node's own action instead of asking who owns that point.
     */
    @Test
    fun a_touch_where_the_faded_pause_button_sits_brings_the_controls_back() {
        openNowPlaying()
        enterFullscreen()
        compose.mainClock.advanceTimeBy(FADED_MS)
        compose.onNodeWithTag("now-playing-controls-faded").assertExists()

        touchThePauseButton()
        compose.mainClock.advanceTimeBy(FRAME_MS)

        compose.onNodeWithTag("now-playing-controls-visible").assertExists()
    }

    /**
     * Progress 0, halfway and settled. Every tap is a coordinate touch: had the button
     * stopped owning its area, the scene behind it would have taken the tap and
     * restarted the idle countdown, and the controls would still be up at the end.
     */
    @Test
    fun the_pause_button_owns_its_area_from_the_first_frame_of_the_transition_to_the_last() {
        openNowPlaying()
        compose.mainClock.autoAdvance = false
        compose.onNodeWithTag("now-playing-enter-fullscreen").performClick()

        compose.mainClock.advanceTimeBy(FRAME_MS)
        touchTheOperablePauseButton()
        compose.mainClock.advanceTimeBy(HALF_TRANSITION_MS)
        touchTheOperablePauseButton()
        compose.mainClock.advanceTimeBy(TRANSITION_SETTLED_MS)
        touchTheOperablePauseButton()

        val sampled = FRAME_MS + HALF_TRANSITION_MS + TRANSITION_SETTLED_MS
        compose.mainClock.advanceTimeBy(IDLE_MS + 4 * FRAME_MS - sampled)
        compose.onNodeWithTag("now-playing-controls-faded").assertExists()
    }

    @Test
    fun the_pause_button_only_changes_size_while_the_transition_runs() {
        openNowPlaying()
        compose.mainClock.autoAdvance = false
        val pause = compose.onNodeWithTag("now-playing-play")
        pause.assertWidthIsEqualTo(PLAYER_PAUSE_SIZE)

        compose.onNodeWithTag("now-playing-enter-fullscreen").performClick()
        compose.mainClock.advanceTimeBy(HALF_TRANSITION_MS)

        val halfway = pause.getUnclippedBoundsInRoot().width
        assertTrue(
            "the pause button must lerp between the two sizes, not swap: $halfway",
            halfway < PLAYER_PAUSE_SIZE && halfway > FULLSCREEN_PAUSE_SIZE,
        )

        compose.mainClock.advanceTimeBy(TRANSITION_SETTLED_MS)
        pause.assertWidthIsEqualTo(FULLSCREEN_PAUSE_SIZE)
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
        openTheSheetIfTheLibraryIsShowing()
    }

    /**
     * Leaves the Now Playing surface open *and settled*. The mini player click only
     * flips the state: the sheet slides in from below over the following frames, so
     * without waiting the scene is not in the tree at all yet. Every caller freezes
     * the clock straight afterwards, which would freeze it before the first frame.
     */
    private fun openTheSheetIfTheLibraryIsShowing() {
        if (compose.onAllNodesWithTag("library-mini-player").fetchSemanticsNodes().isNotEmpty()) {
            compose.onNodeWithTag("library-mini-player").performClick()
            compose.waitForIdle()
        }
    }

    /** Enters the visualizer and lets the transition settle on a paused clock. */
    private fun enterFullscreen() {
        compose.mainClock.autoAdvance = false
        compose.onNodeWithTag("now-playing-enter-fullscreen").performClick()
        compose.mainClock.advanceTimeBy(TRANSITION_SETTLED_MS)
    }

    /** The transition may only resize the button, so it stays operable and keeps its area. */
    private fun touchTheOperablePauseButton() {
        compose.onNodeWithTag("now-playing-play").assertIsEnabled().assertHasClickAction()
        touchThePauseButton()
    }

    /** A coordinate touch, not a semantics click: the question is who owns the spot. */
    private fun touchThePauseButton() {
        compose.onNodeWithTag("now-playing-play").performTouchInput {
            down(center)
            up()
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
