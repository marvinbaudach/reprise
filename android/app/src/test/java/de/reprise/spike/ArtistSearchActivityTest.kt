package de.reprise.spike

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollToNode
import androidx.compose.ui.test.performTextInput
import org.junit.After
import org.junit.Assert.assertEquals
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
class ArtistSearchActivityTest {
    @get:Rule
    val compose = createAndroidComposeRule<MainActivity>()

    private val application: ConfigurationTestApplication
        get() = RuntimeEnvironment.getApplication() as ConfigurationTestApplication

    @After
    fun releaseTheService() {
        application.releaseService()
    }

    @Test
    fun openingAnArtistSearchResultClosesAndClearsTheSearch() {
        compose.onNodeWithText("Artists").performClick()
        compose.onNodeWithContentDescription("Search library").performClick()
        compose.onNodeWithText("Search artists").performTextInput("Artist 45")
        compose.waitForIdle()

        compose.onNodeWithText("Albums").assertDoesNotExist()
        compose.onNodeWithText("Full Album 45").assertDoesNotExist()
        compose.onNodeWithTag("library-artists-list")
            .performScrollToNode(hasText("Artist 45"))
        compose.onNode(
            hasText("Artist 45") and hasText("45 tracks", substring = true),
        ).performClick()

        compose.onNodeWithText("Search artists").assertDoesNotExist()
        compose.onNodeWithContentDescription("Back to artists").performClick()
        compose.waitUntil {
            compose.onAllNodesWithText("Artist 1").fetchSemanticsNodes().isNotEmpty()
        }

        compose.onNodeWithText("Artist 1").assertIsDisplayed()
    }

    @Test
    fun aFailedCatalogReloadAfterOpeningAnArtistRetriesAutomatically() {
        compose.onNodeWithText("Artists").performClick()
        compose.onNodeWithContentDescription("Search library").performClick()
        compose.onNodeWithText("Search artists").performTextInput("Artist 45")
        compose.waitForIdle()

        val attemptsBeforeFailure = application.artistListAttempts.get()
        application.artistListFailuresRemaining = 1
        compose.onNode(
            hasText("Artist 45") and hasText("45 tracks", substring = true),
        ).performClick()
        compose.onNodeWithContentDescription("Back to artists").performClick()
        compose.waitUntil {
            application.artistListAttempts.get() >= attemptsBeforeFailure + 2
        }

        compose.onNodeWithText("Retry").assertDoesNotExist()
        compose.onNodeWithTag("library-artists-list")
            .performScrollToNode(hasText("Artist 1"))
        compose.onNodeWithText("Artist 1").assertIsDisplayed()
    }

    @Test
    fun aPersistentlyFailedCatalogReloadRetriesOnlyOnce() {
        compose.onNodeWithText("Artists").performClick()
        compose.onNodeWithContentDescription("Search library").performClick()
        compose.onNodeWithText("Search artists").performTextInput("Artist 45")
        compose.waitForIdle()

        val attemptsBeforeFailure = application.artistListAttempts.get()
        application.artistListFailuresRemaining = Int.MAX_VALUE
        compose.onNode(
            hasText("Artist 45") and hasText("45 tracks", substring = true),
        ).performClick()
        compose.onNodeWithContentDescription("Back to artists").performClick()
        compose.waitUntil {
            application.artistListAttempts.get() >= attemptsBeforeFailure + 2
        }
        compose.waitForIdle()

        compose.onNodeWithText("Could not load artists:", substring = true).assertIsDisplayed()
        compose.onNodeWithText("Retry").assertDoesNotExist()
        assertEquals(attemptsBeforeFailure + 2, application.artistListAttempts.get())
    }
}
