package de.reprise.spike

import androidx.compose.ui.test.assertHasClickAction
import androidx.compose.ui.test.assertHeightIsAtLeast
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertWidthIsAtLeast
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.unit.dp
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
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
class MobileHeaderRowTest {
    @get:Rule
    val compose = createAndroidComposeRule<MainActivity>()

    private val application: ConfigurationTestApplication
        get() = RuntimeEnvironment.getApplication() as ConfigurationTestApplication

    @After
    fun releaseTheService() {
        application.rememberedDestination = BrowseTab.TITLES
        application.releaseService()
    }

    @Test
    fun noDestinationShowsALibraryAppBarTitle() {
        BrowseTab.entries.forEach { destination ->
            compose.onNodeWithTag("library-destination-${destination.name}").performClick()
            compose.onNodeWithTag("library-page-${destination.name}").assertIsDisplayed()
            compose.onNodeWithTag("library-top-app-bar").assertDoesNotExist()
            compose.onNodeWithText("Library").assertDoesNotExist()
        }
    }

    @Test
    fun everyDestinationKeepsBothActionsWithFortyEightDpTouchTargets() {
        BrowseTab.entries.forEach { destination ->
            compose.onNodeWithTag("library-destination-${destination.name}").performClick()
            compose.onNodeWithTag("library-page-${destination.name}").assertIsDisplayed()

            compose.onNodeWithTag("library-summary-search")
                .assertIsDisplayed()
                .assertHasClickAction()
                .assertWidthIsAtLeast(48.dp)
                .assertHeightIsAtLeast(48.dp)
            compose.onNodeWithTag("library-summary-overflow")
                .assertIsDisplayed()
                .assertHasClickAction()
                .assertWidthIsAtLeast(48.dp)
                .assertHeightIsAtLeast(48.dp)
        }
    }

    @Test
    fun searchFromANonTitleDestinationStillRevealsTheSearchField() {
        compose.onNodeWithTag("library-destination-ARTISTS").performClick()
        compose.onNodeWithTag("library-page-ARTISTS").assertIsDisplayed()

        compose.onNodeWithTag("library-summary-search").performClick()

        compose.onNodeWithTag("library-page-ARTISTS").assertIsDisplayed()
        compose.onNodeWithText("Search albums and artists").assertIsDisplayed()
    }

    @Test
    fun summaryAndActionsShareOneRowWhileActionsStayPutWhenTheSummaryChanges() {
        compose.onNodeWithText("200 of 450 titles loaded").assertIsDisplayed()
        val row = compose.onNodeWithTag("library-summary-row").getUnclippedBoundsInRoot()
        val longSummary = compose.onNodeWithTag("library-summary-text").getUnclippedBoundsInRoot()
        val titlesSearch = compose.onNodeWithTag("library-summary-search")
            .getUnclippedBoundsInRoot()
        val titlesOverflow = compose.onNodeWithTag("library-summary-overflow")
            .getUnclippedBoundsInRoot()

        assertTrue(longSummary.top >= row.top)
        assertTrue(longSummary.bottom <= row.bottom)
        assertTrue(longSummary.right <= titlesSearch.left)
        assertEquals(row.top, titlesSearch.top)
        assertEquals(row.bottom, titlesSearch.bottom)
        assertEquals(row.top, titlesOverflow.top)
        assertEquals(row.bottom, titlesOverflow.bottom)

        compose.onNodeWithTag("library-destination-ARTISTS").performClick()
        compose.onNodeWithTag("library-page-ARTISTS").assertIsDisplayed()
        compose.onNodeWithText("200 of 450 artists loaded").assertIsDisplayed()

        val artistsSearch = compose.onNodeWithTag("library-summary-search")
            .getUnclippedBoundsInRoot()
        val artistsOverflow = compose.onNodeWithTag("library-summary-overflow")
            .getUnclippedBoundsInRoot()
        assertEquals(titlesSearch.left, artistsSearch.left)
        assertEquals(titlesSearch.right, artistsSearch.right)
        assertEquals(titlesOverflow.left, artistsOverflow.left)
        assertEquals(titlesOverflow.right, artistsOverflow.right)
    }
}
