package de.reprise.spike

import android.os.Looper
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsNotDisplayed
import androidx.compose.ui.test.assertIsNotSelected
import androidx.compose.ui.test.assertIsSelected
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
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
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import uniffi.reprise_android_ffi.AndroidStoredLibraryDestination
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
        compose.onNodeWithTag("library-destination-ARTISTS").performClick()
        assertEquals(BrowseTab.ARTISTS, application.rememberedDestination)

        compose.activityRule.scenario.recreate()
        shadowOf(Looper.getMainLooper()).idle()
        application.service.republish()
        shadowOf(Looper.getMainLooper()).idle()
        compose.waitForIdle()

        compose.onNodeWithTag("library-page-ARTISTS").assertIsDisplayed()
        compose.onNodeWithTag("library-destination-ARTISTS").assertIsSelected()
    }

    /**
     * Replaces an earlier rule that no neighbour was to be fetched until it
     * became the visible destination. Opening the library fills rows for the
     * tab it opens on alone — `LibrarySession.browseState` returns the rest
     * `withoutRows()` — and a swipe draws the next page before it settles, so
     * under that rule the first swipe onto a tab showed an empty list and
     * filled it on landing. The window is a bounded 200 rows either way; what
     * the old rule saved was one such query, what it cost was every first
     * swipe. The wait below is what remains of it: the fetch is still not
     * allowed to compete with the opening frames or with a gesture.
     */
    @Test
    fun aNeighbourIsFetchedWhileTheScreenIsStillSoNoSwipeLandsOnAnEmptyList() {
        compose.onNodeWithTag("library-page-TITLES").assertIsDisplayed()
        assertEquals(emptyList<LibraryWindowRange>(), application.artistWindowRequests)

        // Driven by waiting on the effect's own outcome rather than by pushing
        // the clock: the prefetch suspends on a plain `delay`, which the Compose
        // frame clock does not drive.
        compose.waitUntil(timeoutMillis = 10_000) {
            application.artistWindowRequests.isNotEmpty()
        }

        // Fetched before anyone swiped, and fetched exactly once.
        assertEquals(listOf(firstLibraryWindow()), application.artistWindowRequests)

        compose.onNodeWithTag("library-destination-pager").performTouchInput { swipeLeft() }

        compose.onNodeWithTag("library-page-ARTISTS").assertIsDisplayed()
        assertEquals(listOf(firstLibraryWindow()), application.artistWindowRequests)
    }

    @Test
    fun nowPlayingHidesTheNavigationBarAndConsumesThePagerSwipe() {
        application.service.publish(mobileTabsPlayingSnapshot())
        shadowOf(Looper.getMainLooper()).idle()
        compose.waitForIdle()
        compose.onNodeWithTag("library-mini-player").performClick()
        compose.onNodeWithTag("now-playing-transport").assertIsDisplayed()
        compose.onNodeWithTag("library-navigation-bar").assertIsNotDisplayed()

        compose.onNodeWithTag("now-playing-gestures").performTouchInput { swipeLeft() }

        compose.onNodeWithTag("now-playing-transport").assertIsDisplayed()
        compose.onNodeWithTag("library-page-TITLES").assertIsDisplayed()
        compose.onNodeWithTag("library-destination-TITLES").assertIsSelected()
    }
}

@RunWith(RobolectricTestRunner::class)
@Config(
    sdk = [36],
    qualifiers = "w412dp-h916dp-port",
    application = BlockingArtistLoadApplication::class,
)
class CancelledBrowseDoesNotReportFailureTest {
    @get:Rule
    val compose = createAndroidComposeRule<MainActivity>()

    private val application: BlockingArtistLoadApplication
        get() = RuntimeEnvironment.getApplication() as BlockingArtistLoadApplication

    @After
    fun releaseTheService() {
        application.releaseArtistRead.countDown()
        application.releaseService()
    }

    @Test
    fun leavingAVisibleTabWhileItsValidWindowReturnsDoesNotReportAFailure() {
        compose.onNodeWithTag("library-page-ARTISTS").assertIsDisplayed()
        compose.waitUntil(timeoutMillis = 5_000) {
            application.artistReadStarted.count == 0L
        }

        compose.onNodeWithTag("library-destination-QUEUE").performClick()
        compose.onNodeWithTag("library-page-QUEUE").assertIsDisplayed()
        application.releaseArtistRead.countDown()
        compose.waitUntil(timeoutMillis = 5_000) {
            application.artistReadFinished.count == 0L
        }
        compose.waitForIdle()

        compose.onNodeWithText("Could not load artists:", substring = true).assertDoesNotExist()
    }
}

internal class BlockingArtistLoadApplication : ConfigurationTestApplication() {
    val artistReadStarted = CountDownLatch(1)
    val releaseArtistRead = CountDownLatch(1)
    val artistReadFinished = CountDownLatch(1)

    override fun mainActivitySurface(): MainActivitySurfaceDependencies {
        val dependencies = super.mainActivitySurface()
        return dependencies.copy(
            initialStoredDestination = AndroidStoredLibraryDestination.Artists,
            listArtists = { range ->
                artistReadStarted.countDown()
                check(releaseArtistRead.await(10, TimeUnit.SECONDS)) {
                    "timed out waiting to finish the artist window read"
                }
                try {
                    dependencies.listArtists(range)
                } finally {
                    artistReadFinished.countDown()
                }
            },
        )
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
