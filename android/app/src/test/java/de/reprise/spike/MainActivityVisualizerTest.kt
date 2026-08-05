package de.reprise.spike

import android.content.Context
import android.content.Intent
import android.os.Looper
import android.os.PowerManager
import android.view.HapticFeedbackConstants
import android.view.View
import android.view.ViewGroup
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertTextContains
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTouchInput
import androidx.lifecycle.Lifecycle
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
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

@RunWith(RobolectricTestRunner::class)
@Config(
    sdk = [36],
    qualifiers = "w412dp-h916dp-port",
    application = ConfigurationTestApplication::class,
)
@Suppress("DEPRECATION") // ShadowPowerManager's screen-off test hook is deprecated upstream.
class MainActivityVisualizerTest {
    @get:Rule
    val compose = createAndroidComposeRule<MainActivity>()

    private val application: ConfigurationTestApplication
        get() = RuntimeEnvironment.getApplication() as ConfigurationTestApplication

    @After
    fun releaseTheService() {
        application.releaseService()
    }

    @Test
    fun theRealActivityPathSharesOnePersistedFourEntryChoiceWithTheTouchAnchoredMenu() {
        application.service.publish(m9bSnapshot(trackId = 1))
        shadowOf(Looper.getMainLooper()).idle()
        compose.waitForIdle()
        compose.onNodeWithTag("library-mini-player").performClick()

        compose.onNodeWithTag("visualizer-bar-COVER").assertExists()
        compose.onNodeWithTag("visualizer-bar-SPECTRUM").assertIsNotEnabled()
        compose.onNodeWithTag("visualizer-bar-PREVIEW_BAND").assertIsNotEnabled()
        compose.onNodeWithTag("visualizer-bar-AMBIENT").assertExists()

        val surface = compose.onNodeWithTag("visualizer-surface")
        val surfaceBounds = surface.getUnclippedBoundsInRoot()
        surface.performTouchInput {
            down(Offset(width * 0.8f, height * 0.72f))
            advanceEventTime(600)
            up()
        }

        assertEquals(
            HapticFeedbackConstants.LONG_PRESS,
            lastHapticFeedback(compose.activity.window.decorView),
        )
        listOf("Cover", "Spectrum", "Preview", "Ambient").forEach { label ->
            compose.onAllNodesWithText(label).assertCountEquals(2)
        }
        compose.onNodeWithTag("visualizer-menu-SPECTRUM").assertIsNotEnabled()
        compose.onNodeWithTag("visualizer-menu-PREVIEW_BAND").assertIsNotEnabled()
        val anchorBounds = compose.onNodeWithTag("visualizer-menu-anchor").getUnclippedBoundsInRoot()
        assertTrue(anchorBounds.left > (surfaceBounds.left + surfaceBounds.right) / 2f)
        assertTrue(anchorBounds.top > (surfaceBounds.top + surfaceBounds.bottom) / 2f)

        compose.mainClock.autoAdvance = false
        compose.onNodeWithTag("visualizer-menu-AMBIENT").performClick()
        compose.mainClock.advanceTimeBy(60)
        compose.onNodeWithTag("visualizer-cover-surface").assertExists()
        compose.onNodeWithTag("visualizer-ambient-surface").assertExists()
        // One frame establishes the transition before its 120 ms clock starts.
        compose.mainClock.advanceTimeBy(140)
        compose.onNodeWithTag("visualizer-cover-surface").assertDoesNotExist()
        compose.onNodeWithTag("visualizer-ambient-surface").assertExists()
        compose.onNodeWithTag("visualizer-menu-AMBIENT").assertDoesNotExist()
        compose.mainClock.autoAdvance = true
        assertEquals(listOf(MobileVisualizer.AMBIENT), application.visualizerWrites)

        compose.activityRule.scenario.recreate()
        shadowOf(Looper.getMainLooper()).idle()
        application.service.republish()
        shadowOf(Looper.getMainLooper()).idle()
        compose.waitForIdle()
        compose.onNodeWithTag("visualizer-ambient-surface").assertExists()
        assertEquals(listOf(MobileVisualizer.AMBIENT), application.visualizerWrites)

        application.service.publish(m9bSnapshot(trackId = 2))
        shadowOf(Looper.getMainLooper()).idle()
        compose.waitForIdle()
        compose.onNodeWithTag("visualizer-ambient-surface").assertExists()
    }

    @Test
    fun ambientFramesAreUnscheduledOnTheRealPathInBackgroundAndWithTheScreenOff() {
        openAmbient()
        assertEquals(true, application.ambientScheduleEvents.lastOrNull())

        compose.activityRule.scenario.moveToState(Lifecycle.State.STARTED)
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(false, application.ambientScheduleEvents.lastOrNull())

        compose.activityRule.scenario.moveToState(Lifecycle.State.RESUMED)
        shadowOf(Looper.getMainLooper()).idle()
        application.service.republish()
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(true, application.ambientScheduleEvents.lastOrNull())

        val power = compose.activity.getSystemService(Context.POWER_SERVICE) as PowerManager
        shadowOf(power).setIsInteractive(false)
        compose.activity.sendBroadcast(Intent(Intent.ACTION_SCREEN_OFF))
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(false, application.ambientScheduleEvents.lastOrNull())
    }

    @Test
    fun systemAnimationsOffKeepsAmbientStaticOnTheRealActivityPath() {
        openAmbient()
        assertEquals(true, application.ambientScheduleEvents.lastOrNull())
        application.animationsEnabled = false
        application.ambientScheduleEvents.clear()

        compose.activityRule.scenario.recreate()
        shadowOf(Looper.getMainLooper()).idle()
        application.service.republish()
        shadowOf(Looper.getMainLooper()).idle()
        compose.waitForIdle()

        compose.onNodeWithTag("visualizer-ambient-surface").assertExists()
        assertEquals(false, application.ambientScheduleEvents.lastOrNull())
        assertTrue(application.ambientScheduleEvents.none { it })
    }

    /**
     * The bar is on screen the whole time the sheet is; the menu needs a long
     * press nobody is told about. A greyed entry that gives no reason there
     * reads as a failure rather than as something the library has not computed
     * yet — so the reason has to be where the greying is.
     */
    @Test
    fun theAlwaysVisibleBarSaysWhyItsGreyedEntriesCannotBeChosen() {
        application.service.publish(m9bSnapshot(trackId = 1))
        shadowOf(Looper.getMainLooper()).idle()
        compose.waitForIdle()
        compose.onNodeWithTag("library-mini-player").performClick()

        compose.onNodeWithTag("visualizer-bar-SPECTRUM")
            .assertIsNotEnabled()
            .assertTextContains("Needs track analysis")
        compose.onNodeWithTag("visualizer-bar-PREVIEW_BAND")
            .assertIsNotEnabled()
            .assertTextContains("Needs track analysis")
        // And only there: an explanation under every entry would say nothing.
        compose.onNodeWithTag("visualizer-bar-COVER").assertTextEquals("Cover")
        compose.onNodeWithTag("visualizer-bar-AMBIENT").assertTextEquals("Ambient")
    }

    private fun openAmbient() {
        application.service.publish(m9bSnapshot(trackId = 1))
        shadowOf(Looper.getMainLooper()).idle()
        compose.waitForIdle()
        compose.onNodeWithTag("library-mini-player").performClick()
        compose.onNodeWithTag("visualizer-surface").performTouchInput {
            down(center)
            advanceEventTime(600)
            up()
        }
        compose.onNodeWithTag("visualizer-menu-AMBIENT").performClick()
        compose.waitForIdle()
    }
}

private fun lastHapticFeedback(view: View): Int {
    val own = shadowOf(view).lastHapticFeedbackPerformed()
    if (view !is ViewGroup) return own
    return (0 until view.childCount)
        .maxOfOrNull { index -> lastHapticFeedback(view.getChildAt(index)) }
        ?.coerceAtLeast(own)
        ?: own
}

internal fun m9bSnapshot(trackId: Long) = AndroidPlaybackSnapshot(
    state = AndroidPlaybackState.PLAYING,
    currentIndex = 0u,
    currentTrackId = trackId,
    currentTrackUri = "content://provider/document/$trackId.flac",
    positionMs = 12_000,
    durationMs = 120_000,
    shuffled = false,
    repeat = AndroidRepeatMode.OFF,
    error = null,
)
