package io.github.marvinbaudach.reprise

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextInput
import org.junit.After
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(
    sdk = [36],
    qualifiers = "w412dp-h916dp-port",
    application = ConfigurationTestApplication::class,
)
class BrowseDetailLoadingTest {
    @get:Rule
    val compose = createAndroidComposeRule<MainActivity>()

    private val application: ConfigurationTestApplication
        get() = RuntimeEnvironment.getApplication() as ConfigurationTestApplication

    @After
    fun releaseTheService() {
        application.releaseService()
    }

    @Test
    fun anAlbumShowsItsHeaderAndLoadingBodyBeforeItsTracksArrive() {
        openArtistOne()
        application.blockFirstAlbumOpen()

        compose.onNodeWithText("First Album").performClick()
        compose.waitUntil(timeoutMillis = 5_000) { application.firstAlbumOpenHasStarted() }

        compose.onNodeWithContentDescription("Back").assertIsDisplayed()
        compose.onNodeWithText("First Album").assertIsDisplayed()
        compose.onNodeWithText("Artist 1").assertIsDisplayed()
        compose.onNodeWithText("Loading…").assertIsDisplayed()
        compose.onNodeWithText("Artist One · First Album").assertDoesNotExist()

        application.releaseFirstAlbumOpen()
        compose.waitUntil(timeoutMillis = 5_000) { application.firstAlbumOpenHasFinished() }
        compose.waitUntil(timeoutMillis = 5_000) {
            compose.onAllNodesWithText("Artist One · First Album")
                .fetchSemanticsNodes().isNotEmpty()
        }

        compose.onNodeWithText("Loading…").assertDoesNotExist()
        compose.onNodeWithText("Artist One · First Album").assertIsDisplayed()
    }

    @Test
    fun backFromAPendingArtistKeepsItsLateResultFromReopeningTheDetail() {
        compose.onNodeWithText("Artists").performClick()
        application.blockArtistOneOpen()

        compose.onAllNodesWithText("Artist 1")[0].performClick()
        compose.waitUntil(timeoutMillis = 5_000) { application.artistOneOpenHasStarted() }

        compose.onNodeWithContentDescription("Back to artists").assertIsDisplayed()
        compose.onNodeWithText("Loading…").assertIsDisplayed()
        compose.onNodeWithContentDescription("Back to artists").performClick()
        compose.onNodeWithText("Artist 2").assertIsDisplayed()

        application.releaseArtistOneOpen()
        compose.waitUntil(timeoutMillis = 5_000) { application.artistOneOpenHasFinished() }
        compose.waitForIdle()

        compose.onNodeWithContentDescription("Back to artists").assertDoesNotExist()
        compose.onNodeWithText("Loading…").assertDoesNotExist()
        compose.onNodeWithText("Artist 2").assertIsDisplayed()
        compose.onNodeWithText("Albums").assertDoesNotExist()
    }

    @Test
    fun systemBackFromAPendingArtistKeepsItsLateResultFromReopeningTheDetail() {
        compose.onNodeWithText("Artists").performClick()
        application.blockArtistOneOpen()

        compose.onAllNodesWithText("Artist 1")[0].performClick()
        compose.waitUntil(timeoutMillis = 5_000) { application.artistOneOpenHasStarted() }
        compose.runOnIdle { compose.activity.onBackPressedDispatcher.onBackPressed() }
        compose.waitForIdle()

        compose.onNodeWithText("Artist 2").assertIsDisplayed()
        application.releaseArtistOneOpen()
        compose.waitUntil(timeoutMillis = 5_000) { application.artistOneOpenHasFinished() }
        compose.waitForIdle()

        compose.onNodeWithContentDescription("Back to artists").assertDoesNotExist()
        compose.onNodeWithText("Loading…").assertDoesNotExist()
        compose.onNodeWithText("Artist 2").assertIsDisplayed()
        compose.onNodeWithText("Albums").assertDoesNotExist()
    }

    @Test
    fun systemBackFromAPendingAlbumKeepsItsLateResultFromReopeningTheDetail() {
        openArtistOne()
        application.blockFirstAlbumOpen()

        compose.onNodeWithText("First Album").performClick()
        compose.waitUntil(timeoutMillis = 5_000) { application.firstAlbumOpenHasStarted() }
        compose.runOnIdle { compose.activity.onBackPressedDispatcher.onBackPressed() }
        compose.waitForIdle()

        compose.onNodeWithContentDescription("Back to artists").assertIsDisplayed()
        compose.onNodeWithText("First Album").assertIsDisplayed()
        application.releaseFirstAlbumOpen()
        compose.waitUntil(timeoutMillis = 5_000) { application.firstAlbumOpenHasFinished() }
        compose.waitForIdle()

        compose.onNodeWithContentDescription("Back").assertDoesNotExist()
        compose.onNodeWithText("Loading…").assertDoesNotExist()
        compose.onNodeWithText("Artist One · First Album").assertDoesNotExist()
        compose.onNodeWithContentDescription("Back to artists").assertIsDisplayed()
        compose.onNodeWithText("First Album").assertIsDisplayed()
    }

    @Test
    fun aPendingArtistOpenDoesNotDiscardTheSearchResultThatWasAlreadyRequested() {
        compose.onNodeWithText("Artists").performClick()
        application.blockArtist45SearchAndCatchUp()
        application.blockArtistOneOpen()

        compose.onNodeWithContentDescription("Search library").performClick()
        compose.onNodeWithText("Search artists").performTextInput("Artist 45")
        compose.waitUntil(timeoutMillis = 5_000) { application.artist45SearchHasStarted() }
        compose.onAllNodesWithText("Artist 1")[0].performClick()
        compose.waitUntil(timeoutMillis = 5_000) { application.artistOneOpenHasStarted() }

        application.releaseArtist45Search()
        compose.waitUntil(timeoutMillis = 5_000) { application.artist45SearchHasFinished() }
        compose.onNodeWithContentDescription("Back to artists").performClick()
        compose.waitForIdle()

        compose.onNode(
            hasText("Artist 45") and hasText("45 tracks", substring = true),
        ).assertIsDisplayed()
        compose.onNodeWithText("Artist 2").assertDoesNotExist()

        application.releaseArtist45SearchCatchUp()
        application.releaseArtistOneOpen()
    }

    private fun openArtistOne() {
        compose.onNodeWithText("Artists").performClick()
        compose.onAllNodesWithText("Artist 1")[0].performClick()
        compose.waitUntil(timeoutMillis = 5_000) {
            compose.onAllNodesWithText("First Album").fetchSemanticsNodes().isNotEmpty()
        }
    }
}
