package io.github.marvinbaudach.reprise

import androidx.compose.ui.test.assertHasClickAction
import androidx.compose.ui.test.assertHeightIsAtLeast
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertWidthIsAtLeast
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollToIndex
import androidx.compose.ui.unit.dp
import androidx.lifecycle.ViewModelProvider
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
    fun everyDestinationKeepsTheOverflowActionAndOnlyQueueDropsTheSearchAction() {
        BrowseTab.entries.forEach { destination ->
            compose.onNodeWithTag("library-destination-${destination.name}").performClick()
            compose.onNodeWithTag("library-page-${destination.name}").assertIsDisplayed()

            if (destination == BrowseTab.QUEUE) {
                compose.onNodeWithTag("library-summary-search").assertDoesNotExist()
            } else {
                compose.onNodeWithTag("library-summary-search")
                    .assertIsDisplayed()
                    .assertHasClickAction()
                    .assertWidthIsAtLeast(48.dp)
                    .assertHeightIsAtLeast(48.dp)
            }
            compose.onNodeWithTag("library-summary-overflow")
                .assertIsDisplayed()
                .assertHasClickAction()
                .assertWidthIsAtLeast(48.dp)
                .assertHeightIsAtLeast(48.dp)
        }
    }

    @Test
    fun queueOffersNoSearchActionSoNoFieldCanEverOpenThere() {
        compose.onNodeWithTag("library-destination-QUEUE").performClick()
        compose.onNodeWithTag("library-page-QUEUE").assertIsDisplayed()

        compose.onNodeWithTag("library-summary-search").assertDoesNotExist()
        compose.onNodeWithText("Search queue").assertDoesNotExist()
    }

    @Test
    fun searchFromANonTitleDestinationStillRevealsTheSearchField() {
        compose.onNodeWithTag("library-destination-ARTISTS").performClick()
        compose.onNodeWithTag("library-page-ARTISTS").assertIsDisplayed()

        compose.onNodeWithTag("library-summary-search").performClick()

        compose.onNodeWithTag("library-page-ARTISTS").assertIsDisplayed()
        compose.onNodeWithText("Search artists").assertIsDisplayed()
    }

    @Test
    fun summaryAndActionsShareOneRowWhileActionsStayPutWhenTheSummaryChanges() {
        compose.onNodeWithText("450 titles").assertIsDisplayed()
        val row = compose.onNodeWithTag("library-summary-row").getUnclippedBoundsInRoot()
        val titlesSummary = compose.onNodeWithTag("library-summary-text").getUnclippedBoundsInRoot()
        val titlesSearch = compose.onNodeWithTag("library-summary-search")
            .getUnclippedBoundsInRoot()
        val titlesOverflow = compose.onNodeWithTag("library-summary-overflow")
            .getUnclippedBoundsInRoot()

        assertTrue(titlesSummary.top >= row.top)
        assertTrue(titlesSummary.bottom <= row.bottom)
        assertTrue(titlesSummary.right <= titlesSearch.left)
        assertEquals(row.top, titlesSearch.top)
        assertEquals(row.bottom, titlesSearch.bottom)
        assertEquals(row.top, titlesOverflow.top)
        assertEquals(row.bottom, titlesOverflow.bottom)

        compose.onNodeWithTag("library-destination-ARTISTS").performClick()
        compose.onNodeWithTag("library-page-ARTISTS").assertIsDisplayed()
        compose.onNodeWithText("450 artists").assertIsDisplayed()

        val artistsSearch = compose.onNodeWithTag("library-summary-search")
            .getUnclippedBoundsInRoot()
        val artistsOverflow = compose.onNodeWithTag("library-summary-overflow")
            .getUnclippedBoundsInRoot()
        assertEquals(titlesSearch.left, artistsSearch.left)
        assertEquals(titlesSearch.right, artistsSearch.right)
        assertEquals(titlesOverflow.left, artistsOverflow.left)
        assertEquals(titlesOverflow.right, artistsOverflow.right)

        compose.onNodeWithTag("library-artists-list").performScrollToIndex(198)
        compose.onNodeWithText("Artist 199").performClick()
        compose.onNodeWithText("1 album • 199 tracks").assertIsDisplayed()

        val artistDetailRow = compose.onNodeWithTag("library-summary-row")
            .getUnclippedBoundsInRoot()
        val artistDetailSummary = compose.onNodeWithTag("library-summary-text")
            .getUnclippedBoundsInRoot()
        val artistDetailSearch = compose.onNodeWithTag("library-summary-search")
            .getUnclippedBoundsInRoot()
        val artistDetailOverflow = compose.onNodeWithTag("library-summary-overflow")
            .getUnclippedBoundsInRoot()
        assertTrue(artistDetailSummary.top >= artistDetailRow.top)
        assertTrue(artistDetailSummary.bottom <= artistDetailRow.bottom)
        assertTrue(artistDetailSummary.right <= artistDetailSearch.left)
        assertEquals(titlesSearch.left, artistDetailSearch.left)
        assertEquals(titlesSearch.right, artistDetailSearch.right)
        assertEquals(titlesOverflow.left, artistDetailOverflow.left)
        assertEquals(titlesOverflow.right, artistDetailOverflow.right)
    }

    @Test
    fun artworkProgressChangesTheSummaryWithoutChangingTheHeaderRow() {
        val rowBefore = compose.onNodeWithTag("library-summary-row").getUnclippedBoundsInRoot()
        val searchBefore = compose.onNodeWithTag("library-summary-search").getUnclippedBoundsInRoot()
        val overflowBefore = compose.onNodeWithTag("library-summary-overflow")
            .getUnclippedBoundsInRoot()
        val surface = ViewModelProvider(compose.activity)[MobileSurfaceViewModel::class.java]

        compose.runOnIdle {
            surface.acceptArtistPhotoProgress(
                ArtistPhotoProgress(4, ArtistPhotoProgressPhase.RUNNING, 2, 0, 6),
            )
        }

        compose.onNodeWithText("450 titles · Artwork 2/6").assertIsDisplayed()
        assertEquals(rowBefore, compose.onNodeWithTag("library-summary-row").getUnclippedBoundsInRoot())
        assertEquals(
            searchBefore,
            compose.onNodeWithTag("library-summary-search").getUnclippedBoundsInRoot(),
        )
        assertEquals(
            overflowBefore,
            compose.onNodeWithTag("library-summary-overflow").getUnclippedBoundsInRoot(),
        )
    }
}
