package de.reprise.spike

import android.app.Application
import android.os.Looper
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.assertHeightIsEqualTo
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsOff
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isToggleable
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.unit.dp
import androidx.lifecycle.ViewModelProvider
import org.junit.After
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

/**
 * Settings navigation claims run through the same path as a device:
 * [MainActivity.onCreate], the production [BrowseScreen] overlay, and the
 * activity-owned [MobileSurfaceViewModel]. Directly mounting the settings
 * composable would not cover that lifecycle wiring.
 */
@RunWith(RobolectricTestRunner::class)
@Config(
    sdk = [36],
    qualifiers = "w412dp-h916dp-port",
    application = ConfigurationTestApplication::class,
)
class MainActivitySettingsNavigationTest {
    @get:Rule
    val compose = createAndroidComposeRule<MainActivity>()

    private val application: ConfigurationTestApplication
        get() = RuntimeEnvironment.getApplication() as ConfigurationTestApplication

    @After
    fun releaseTheService() {
        application.releaseService()
    }

    @Test
    fun overviewListsExactlyTheFiveSectionsThatExist() {
        openSettings()

        val rows = compose.onAllNodesWithTag("settings-overview-row")
        rows.assertCountEquals(5)
        repeat(5) { index -> rows[index].assertHeightIsEqualTo(72.dp) }
        compose.onNodeWithText("Library & scan folder").assertIsDisplayed()
        compose.onNodeWithText("450 titles · 1 folder").assertIsDisplayed()
        compose.onNodeWithText("Audio").assertIsDisplayed()
        compose.onNodeWithText("Gapless, Equalizer").assertIsDisplayed()
        compose.onNodeWithText("Appearance").assertIsDisplayed()
        compose.onNodeWithText("Nocturne").assertIsDisplayed()
        compose.onNodeWithText("Online sources").assertIsDisplayed()
        compose.onNodeWithText("Off").assertIsDisplayed()
        compose.onNodeWithText("About Reprise").assertIsDisplayed()
        compose.onNodeWithText(BuildConfig.VERSION_NAME).assertIsDisplayed()
        compose.onNodeWithText("Sync & devices").assertDoesNotExist()
    }

    @Test
    fun theOnlineSourcesPageOpensAndBackReturnsToTheOverview() {
        openSettings()

        compose.onNodeWithContentDescription("Open Online sources").performClick()
        compose.onNodeWithTag("settings-page-online-sources").assertIsDisplayed()
        compose.onAllNodesWithTag("settings-overview-row").assertCountEquals(0)

        compose.onNodeWithContentDescription("Back to Settings").performClick()

        compose.onAllNodesWithTag("settings-overview-row").assertCountEquals(5)
        compose.onNodeWithText("Online sources").assertIsDisplayed()
    }

    @Test
    fun aFailedOnlineSourcesWriteKeepsTheSwitchAndOverviewOff() {
        application.onlineSourcesWriteSucceeds = false
        openSettings()
        compose.onNodeWithContentDescription("Open Online sources").performClick()

        compose.onNode(hasText("Download artist photos") and isToggleable())
            .assertIsOff()
            .performClick()

        assertFalse(application.onlineSourcesEnabled)
        compose.onNode(hasText("Download artist photos") and isToggleable()).assertIsOff()
        compose.onNodeWithContentDescription("Back to Settings").performClick()
        compose.onNodeWithText("Off").assertIsDisplayed()
    }

    @Test
    fun pageBackReturnsToOverviewAndOnlyOverviewBackClosesTheOverlay() {
        openSettings()
        compose.onNodeWithContentDescription("Open Audio").performClick()
        compose.onAllNodesWithTag("settings-overview-row").assertCountEquals(0)
        compose.onNodeWithText("Gapless playback").assertIsDisplayed()

        compose.activity.onBackPressedDispatcher.onBackPressed()
        compose.waitForIdle()

        compose.onAllNodesWithTag("settings-overview-row").assertCountEquals(5)
        assertTrue(surfaceState().settingsVisible)

        compose.activity.onBackPressedDispatcher.onBackPressed()
        compose.waitForIdle()

        assertFalse(surfaceState().settingsVisible)
        compose.onNodeWithContentDescription("Library actions").assertIsDisplayed()
        compose.onAllNodesWithTag("settings-overview-row").assertCountEquals(0)
    }

    @Test
    fun audioPageSurvivesActivityScenarioRecreate() {
        openSettings()
        compose.onNodeWithContentDescription("Open Audio").performClick()
        compose.onNodeWithTag("settings-page-audio").assertIsDisplayed()

        compose.activityRule.scenario.recreate()
        shadowOf(Looper.getMainLooper()).idle()
        application.service.republish()
        shadowOf(Looper.getMainLooper()).idle()
        compose.waitForIdle()

        compose.onNodeWithTag("settings-page-audio").assertIsDisplayed()
        compose.onAllNodesWithTag("settings-overview-row").assertCountEquals(0)
    }

    private fun openSettings() {
        compose.onNodeWithContentDescription("Library actions").performClick()
        compose.onNodeWithText("Settings").performClick()
    }

    private fun surfaceState(): MobileSurfaceViewModel =
        ViewModelProvider(compose.activity)[MobileSurfaceViewModel::class.java]
}
