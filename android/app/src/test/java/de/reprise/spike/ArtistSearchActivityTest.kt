package de.reprise.spike

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
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
    fun artistSearchQueriesBothKindsAndOpensAnAlbumDirectly() {
        compose.onNodeWithText("Artists").performClick()
        compose.onNodeWithContentDescription("Search library").performClick()
        compose.onNodeWithText("Search albums and artists").performTextInput("artist 2")
        compose.waitForIdle()

        val album = compose.onNodeWithText("Full Album 2")
        val artist = compose.onNodeWithText("Artist 2")
        album.assertIsDisplayed()
        compose.onNodeWithTag("library-artist-search-albums-list")
            .performScrollToNode(hasText("Artist 2"))
        artist.assertIsDisplayed()
        compose.onNodeWithTag("library-artist-search-albums-list")
            .performScrollToNode(hasText("Full Album 2"))

        album.performClick()

        compose.onNodeWithContentDescription("Back to albums").assertIsDisplayed()
        compose.onNodeWithText("Album Song 1").assertIsDisplayed()
    }
}
