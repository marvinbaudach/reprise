package de.reprise.spike

import android.os.Looper
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
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

/**
 * M11 library-place claims run through [MainActivity.onCreate] and the real
 * activity-owned [MobileSurfaceViewModel]. The only replaced edge is the
 * native library/playback boundary supplied by [ConfigurationTestApplication].
 */
@RunWith(RobolectricTestRunner::class)
@Config(
    sdk = [36],
    qualifiers = "w412dp-h916dp-port",
    application = ConfigurationTestApplication::class,
)
class MainActivityMusicPathsTest {
    @get:Rule
    val compose = createAndroidComposeRule<MainActivity>()

    private val application: ConfigurationTestApplication
        get() = RuntimeEnvironment.getApplication() as ConfigurationTestApplication

    @After
    fun releaseTheService() {
        application.releaseService()
    }

    @Test
    fun artistTileOpensItsAlbumsAndNestedAlbumSurvivesRecreate() {
        compose.onNodeWithText("Artists").performClick()
        compose.onAllNodesWithText("Artist 1")[0].performClick()

        compose.onNodeWithContentDescription("Back to artists").assertIsDisplayed()
        assertAbove("First Album", "Second Album")
        compose.onNodeWithText("Artist 1 • First Album").assertDoesNotExist()
        compose.onNodeWithText("Someone Else · Album").assertDoesNotExist()
        compose.onNodeWithText("First Album").performClick()
        compose.onNodeWithContentDescription("Back").assertIsDisplayed()
        compose.onNodeWithText("Artist One · First Album").assertIsDisplayed()

        compose.activityRule.scenario.recreate()
        shadowOf(Looper.getMainLooper()).idle()
        application.service.republish()
        shadowOf(Looper.getMainLooper()).idle()
        compose.waitForIdle()

        compose.onNodeWithContentDescription("Back").assertIsDisplayed()
        compose.onNodeWithText("Artist One · First Album").assertIsDisplayed()
    }

    @Test
    fun listPlayButtonsReplaceTheQueueAndStartAtTheFirstTrack() {
        application.replaceQueue(listOf(configurationTestTrack(99, "Old Queue Track")))

        openDeepAlbum()
        compose.onNodeWithContentDescription("Play Deep Album").performClick()

        assertEquals("Album Song 1", application.currentQueue.first().title)
        assertEquals(0, application.currentQueueIndex)
        assertEquals(200, application.currentQueue.size)

        compose.onNodeWithContentDescription("Back").performClick()
        compose.onNodeWithText("First Album").performClick()
        compose.onNodeWithContentDescription("Play First Album").performClick()

        assertEquals("Artist One · First Album", application.currentQueue.first().title)
        assertEquals(0, application.currentQueueIndex)
        assertEquals(1, application.currentQueue.size)
    }

    @Test
    fun backClosesTheTopSurfaceThenOpenAlbumAndArtistWithoutLeavingTheActivity() {
        openDeepAlbum()
        compose.waitForIdle()
        compose.activityRule.scenario.recreate()
        shadowOf(Looper.getMainLooper()).idle()
        application.service.republish()
        shadowOf(Looper.getMainLooper()).idle()
        compose.waitForIdle()
        compose.onNodeWithContentDescription("Back").assertIsDisplayed()

        application.service.publish(m9bSnapshot(1))
        shadowOf(Looper.getMainLooper()).idle()
        compose.waitForIdle()
        compose.onNodeWithTag("library-mini-player").performClick()
        compose.onNodeWithTag("now-playing-transport").assertIsDisplayed()

        compose.activity.onBackPressedDispatcher.onBackPressed()
        compose.waitForIdle()

        compose.onNodeWithTag("now-playing-transport").assertDoesNotExist()
        compose.onNodeWithContentDescription("Back").assertIsDisplayed()

        compose.activity.onBackPressedDispatcher.onBackPressed()
        compose.waitForIdle()

        assertEquals(Lifecycle.State.RESUMED, compose.activityRule.scenario.state)
        compose.onNodeWithContentDescription("Back").assertDoesNotExist()
        compose.onNodeWithText("Deep Album").assertIsDisplayed()
        compose.onNodeWithContentDescription("Back to artists").performClick()

        compose.onAllNodesWithText("Artist 1")[0].performClick()
        compose.waitForIdle()
        compose.activityRule.scenario.recreate()
        shadowOf(Looper.getMainLooper()).idle()
        application.service.republish()
        shadowOf(Looper.getMainLooper()).idle()
        compose.waitForIdle()
        compose.onNodeWithContentDescription("Back to artists").assertIsDisplayed()

        compose.activity.onBackPressedDispatcher.onBackPressed()
        compose.waitForIdle()

        assertEquals(Lifecycle.State.RESUMED, compose.activityRule.scenario.state)
        compose.onNodeWithContentDescription("Back to artists").assertDoesNotExist()
        compose.onAllNodesWithText("Artist 1")[0].assertIsDisplayed()
    }

    private fun openDeepAlbum() {
        compose.onNodeWithText("Artists").performClick()
        compose.onAllNodesWithText("Artist 1")[0].performClick()
        compose.onNodeWithText("Deep Album").performClick()
    }

    private fun assertAbove(upperText: String, lowerText: String) {
        val upper = compose.onNodeWithText(upperText)
        val lower = compose.onNodeWithText(lowerText)
        upper.assertIsDisplayed()
        lower.assertIsDisplayed()
        assertTrue(
            "$upperText must stay above $lowerText",
            upper.getUnclippedBoundsInRoot().top < lower.getUnclippedBoundsInRoot().top,
        )
    }
}
