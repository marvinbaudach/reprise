package de.reprise.spike

import android.os.Looper
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsNotSelected
import androidx.compose.ui.test.assertIsSelected
import androidx.compose.ui.test.click
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.swipeLeft
import org.junit.After
import org.junit.Assert.assertEquals
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
class MobileBottomTabsTest {
    @get:Rule
    val compose = createAndroidComposeRule<MainActivity>()

    private val application: ConfigurationTestApplication
        get() = RuntimeEnvironment.getApplication() as ConfigurationTestApplication

    @After
    fun releaseTheService() {
        application.releaseService()
    }

    @Test
    fun tappingADestinationShowsItsPageAndLeavesExactlyOneDestinationSelected() {
        compose.onNodeWithTag("library-destination-ARTISTS").performClick()

        compose.onNodeWithTag("library-page-ARTISTS").assertIsDisplayed()
        compose.onNodeWithTag("library-destination-TITLES").assertIsNotSelected()
        compose.onNodeWithTag("library-destination-ARTISTS").assertIsSelected()
        compose.onNodeWithTag("library-destination-ALBUMS").assertIsNotSelected()
        compose.onNodeWithTag("library-destination-FAVOURITES").assertIsNotSelected()
        compose.onNodeWithTag("library-destination-QUEUE").assertIsNotSelected()
    }

    @Test
    fun swipingLeftShowsTheNextPageAndMovesTheDestinationSelection() {
        compose.onNodeWithTag("library-destination-pager").performTouchInput { swipeLeft() }

        compose.onNodeWithTag("library-page-ARTISTS").assertIsDisplayed()
        compose.onNodeWithTag("library-destination-TITLES").assertIsNotSelected()
        compose.onNodeWithTag("library-destination-ARTISTS").assertIsSelected()
    }

    @Test
    fun swipingPastTheLastDestinationStopsOnQueue() {
        compose.onNodeWithTag("library-destination-QUEUE").performClick()
        compose.onNodeWithTag("library-destination-pager").performTouchInput { swipeLeft() }

        compose.onNodeWithTag("library-page-QUEUE").assertIsDisplayed()
        compose.onNodeWithTag("library-destination-QUEUE").assertIsSelected()
    }

    @Test
    fun theChosenDestinationIsRememberedAndSurvivesActivityRecreation() {
        compose.onNodeWithTag("library-destination-ALBUMS").performClick()
        assertEquals(BrowseTab.ALBUMS, application.rememberedDestination)

        compose.activityRule.scenario.recreate()
        shadowOf(Looper.getMainLooper()).idle()
        application.service.republish()
        shadowOf(Looper.getMainLooper()).idle()
        compose.waitForIdle()

        compose.onNodeWithTag("library-page-ALBUMS").assertIsDisplayed()
        compose.onNodeWithTag("library-destination-ALBUMS").assertIsSelected()
    }

    @Test
    fun aComposedNeighbourRequestsNoWindowUntilItBecomesTheVisibleDestination() {
        compose.onNodeWithTag("library-page-TITLES").assertIsDisplayed()
        assertEquals(emptyList<LibraryWindowRange>(), application.artistWindowRequests)

        compose.onNodeWithTag("library-destination-pager").performTouchInput { swipeLeft() }

        compose.onNodeWithTag("library-page-ARTISTS").assertIsDisplayed()
        assertEquals(listOf(firstLibraryWindow()), application.artistWindowRequests)
    }

    @Test
    fun nowPlayingLeavesTheNavigationBarVisibleAndReceivingTheDestinationTap() {
        application.service.publish(mobileTabsPlayingSnapshot())
        shadowOf(Looper.getMainLooper()).idle()
        compose.waitForIdle()
        compose.onNodeWithTag("library-mini-player").performClick()
        compose.onNodeWithContentDescription("Collapse Now Playing").assertIsDisplayed()
        compose.onNodeWithTag("library-navigation-bar").assertIsDisplayed()

        compose.onNodeWithTag("library-destination-ARTISTS")
            .performTouchInput { click() }

        compose.onNodeWithContentDescription("Collapse Now Playing").assertDoesNotExist()
        compose.onNodeWithTag("library-page-ARTISTS").assertIsDisplayed()
        compose.onNodeWithTag("library-destination-ARTISTS").assertIsSelected()
    }
}

private fun mobileTabsPlayingSnapshot() = AndroidPlaybackSnapshot(
    state = AndroidPlaybackState.PLAYING,
    currentIndex = 0u,
    currentTrackId = 1,
    currentTrackUri = "content://provider/document/1.flac",
    positionMs = 12_000,
    durationMs = 120_000,
    automaticAdvanceCount = 0u,
    shuffled = false,
    repeat = AndroidRepeatMode.OFF,
    error = null,
)
