package de.reprise.spike

import android.os.Looper
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertContentDescriptionEquals
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

/**
 * The M11 heart runs through [MainActivity.onCreate], the activity surface,
 * its accepted-rating store, and [androidx.test.core.app.ActivityScenario.recreate].
 */
@RunWith(RobolectricTestRunner::class)
@Config(
    sdk = [36],
    qualifiers = "w412dp-h916dp-port",
    application = ConfigurationTestApplication::class,
)
class MainActivityRatingTest {
    @get:Rule
    val compose = createAndroidComposeRule<MainActivity>()

    private val application: ConfigurationTestApplication
        get() = RuntimeEnvironment.getApplication() as ConfigurationTestApplication

    @After
    fun releaseTheService() {
        application.releaseService()
    }

    @Test
    fun libraryHeartWritesFiveThenZeroAndBothSurviveAFreshActivityRead() {
        libraryHeart("Add to favourites").performClick()
        compose.waitForIdle()

        assertEquals(listOf(1L to 5), application.controls.ratingRequests)
        assertEquals(5, application.trackRatings[1L])
        assertEquals(emptyList<LibraryTrack>(), application.currentQueue)

        compose.onNodeWithText("Favourites").performClick()
        favouriteTrack().assertIsDisplayed()
        libraryHeart("Remove from favourites").performClick()
        compose.waitForIdle()

        assertEquals(listOf(1L to 5, 1L to 0), application.controls.ratingRequests)
        assertEquals(0, application.trackRatings[1L])
        favouriteTrack().assertDoesNotExist()

        recreate()
        favouriteTrack().assertDoesNotExist()
    }

    @Test
    fun sheetDockAndLibraryReadOneHeartWithoutRestoreMemory() {
        publishTrack(1)
        compose.onNodeWithTag("library-mini-player").performClick()
        compose.onNodeWithTag("now-playing-heart").performClick()
        compose.waitForIdle()

        assertEquals(listOf(1L to 5), application.controls.ratingRequests)
        compose.onNodeWithContentDescription("Collapse Now Playing").performClick()
        libraryHeart("Remove from favourites").assertIsDisplayed()

        recreateAt("w916dp-h412dp-land")
        compose.onNodeWithText("Dock mode").performClick()
        compose.onNodeWithTag("dock-heart").performClick()
        compose.waitForIdle()

        assertEquals(listOf(1L to 5, 1L to 0), application.controls.ratingRequests)
        compose.onNodeWithContentDescription("Exit dock mode").performClick()
        compose.onNodeWithTag("now-playing-heart")
            .assertContentDescriptionEquals("Add to favourites")
    }

    @Test
    fun refusedHeartWriteMovesNoSurfaceAndExplainsTheFailure() {
        application.controls.ratingFailure = RATING_REFUSED

        libraryHeart("Add to favourites").performClick()
        compose.waitForIdle()

        assertEquals(listOf(1L to 5), application.controls.ratingRequests)
        libraryHeart("Add to favourites").assertIsDisplayed()
        compose.onNodeWithText(RATING_REFUSED).assertIsDisplayed()
    }

    private fun libraryHeart(description: String) =
        compose.onNode(
            hasTestTag(TRACK_HEART_TAG) and
                hasContentDescription(description) and
                hasAnyAncestor(hasTestTag("library-track-row-1")) and
                hasAnyAncestor(
                    hasTestTag("library-page-${application.rememberedDestination.name}"),
                ),
        )
            .assertContentDescriptionEquals(description)

    private fun favouriteTrack() = compose.onNode(
        hasTestTag("library-track-row-1") and
            hasAnyAncestor(hasTestTag("library-page-FAVOURITES")),
    )

    private fun publishTrack(trackId: Long) {
        application.service.publish(m9bSnapshot(trackId))
        shadowOf(Looper.getMainLooper()).idle()
        compose.waitForIdle()
    }

    private fun recreate() {
        compose.activityRule.scenario.recreate()
        shadowOf(Looper.getMainLooper()).idle()
        application.service.republish()
        shadowOf(Looper.getMainLooper()).idle()
        compose.waitForIdle()
    }

    private fun recreateAt(qualifiers: String) {
        RuntimeEnvironment.setQualifiers(qualifiers)
        recreate()
    }
}

private const val RATING_REFUSED = "Could not save rating: track is missing."
