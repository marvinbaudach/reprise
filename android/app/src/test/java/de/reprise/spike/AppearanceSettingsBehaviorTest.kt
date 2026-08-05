package de.reprise.spike

import android.os.Looper
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTouchInput
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

/** The settings page and the Now Playing menu are two controls over one value. */
@RunWith(RobolectricTestRunner::class)
@Config(
    sdk = [36],
    qualifiers = "w412dp-h916dp-port",
    application = ConfigurationTestApplication::class,
)
class AppearanceSettingsBehaviorTest {
    @get:Rule
    val compose = createAndroidComposeRule<MainActivity>()

    private val application: ConfigurationTestApplication
        get() = RuntimeEnvironment.getApplication() as ConfigurationTestApplication

    @After
    fun releaseTheService() {
        application.releaseService()
    }

    @Test
    fun settingsAndLongPressMenuReadAndWriteTheSameVisualizerValueBothWays() {
        application.service.publish(m9bSnapshot(trackId = 1))
        shadowOf(Looper.getMainLooper()).idle()
        compose.waitForIdle()

        openAppearanceSettings()
        compose.onNodeWithTag("settings-visualizer-SPECTRUM").assertIsNotEnabled()
        compose.onNodeWithTag("settings-visualizer-PREVIEW_BAND").assertIsNotEnabled()
        compose.onNodeWithTag("settings-visualizer-AMBIENT").performClick()
        assertEquals(listOf(MobileVisualizer.AMBIENT), application.visualizerWrites)

        closeSettings()
        compose.onNodeWithTag("library-mini-player").performClick()
        compose.onNodeWithTag("visualizer-bar-AMBIENT").assertSelected()

        compose.onNodeWithTag("visualizer-surface").performTouchInput {
            down(Offset(width * 0.8f, height * 0.72f))
            advanceEventTime(600)
            up()
        }
        compose.onNodeWithTag("visualizer-menu-COVER").performClick()
        compose.onNodeWithContentDescription("Collapse Now Playing").performClick()

        openAppearanceSettings()
        compose.onNodeWithTag("settings-visualizer-COVER").assertSelected()
        assertEquals(
            listOf(MobileVisualizer.AMBIENT, MobileVisualizer.COVER),
            application.visualizerWrites,
        )
    }

    private fun openAppearanceSettings() {
        compose.onNodeWithContentDescription("Library actions").performClick()
        compose.onNodeWithText("Settings").performClick()
        compose.onNodeWithContentDescription("Open Appearance").performClick()
    }

    private fun closeSettings() {
        compose.onNodeWithContentDescription("Back to Settings").performClick()
        compose.onNodeWithContentDescription("Back to Library").performClick()
    }

    private fun androidx.compose.ui.test.SemanticsNodeInteraction.assertSelected() =
        assert(SemanticsMatcher.expectValue(SemanticsProperties.Selected, true))
}
