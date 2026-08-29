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
}
