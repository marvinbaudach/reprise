package de.reprise.spike

import android.os.Looper
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.SemanticsNodeInteraction
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.assertTextContains
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.semantics.SemanticsProperties
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
 * One rating, one copy of it.
 *
 * A track's rating is shown by three surfaces — the library row, the sheet's
 * five stars and the dock's single one. Each of them used to hold its own
 * `remember`ed copy seeded from the row it was handed and moved only by its
 * *own* successful write, and nothing re-reads the playing row after a rating
 * write, so the copies drifted apart the first time two surfaces were used in
 * turn. These tests take that turn on the real activity: they never build a
 * composable by hand, and every rating goes through the activity's own writer.
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

    /**
     * The reachable path, and it needs no re-entry into the dock: ✕ is not a
     * configuration change, so the sheet that comes back is the sheet that was
     * there, and the row behind it is the one loaded before the dock was
     * entered. Nothing about this asks the database again.
     */
    @Test
    fun aStarTappedInTheDockIsTheRatingTheSheetShowsAfterTheExit() {
        publishTrack(1)
        assertEquals(2, application.trackRatings[1L] ?: 2)
        recreateAt("w916dp-h412dp-land")

        compose.onNodeWithText("Dock mode").performClick()
        compose.waitForIdle()
        compose.onNodeWithTag("dock-star").performClick()
        compose.waitForIdle()

        assertEquals(listOf(1L to 5), application.controls.ratingRequests)
        assertEquals(5, application.trackRatings[1L])

        compose.onNodeWithContentDescription("Exit dock mode").performClick()
        compose.waitForIdle()

        compose.onNodeWithTag("dock-surface").assertDoesNotExist()
        (1..5).forEach { star -> star(star).assertRating(5) }
    }

    /**
     * The same root, one surface further out and older than the dock: the row
     * a window was paged in with is a copy of the track, and rating the playing
     * track never rewrote it.
     */
    @Test
    fun aStarTappedInTheSheetIsTheRatingTheLibraryRowShowsAfterItCloses() {
        publishTrack(1)
        compose.onNodeWithTag("library-track-row-1").assertTextContains("2/5")

        compose.onNodeWithTag("library-mini-player").performClick()
        compose.waitForIdle()
        star(4).performClick()
        compose.waitForIdle()

        assertEquals(listOf(1L to 4), application.controls.ratingRequests)
        star(4).assertRating(4)

        compose.onNodeWithContentDescription("Collapse Now Playing").performClick()
        compose.waitForIdle()

        compose.onNodeWithTag("library-track-row-1").assertTextContains("4/5")
    }

    /**
     * The property both reviews singled out, now that one place answers for all
     * three surfaces: moving that one place early would move every star at once,
     * so a refused write has to leave all of them exactly where they were.
     */
    @Test
    fun aRefusedWriteMovesNoStarOnAnySurface() {
        application.controls.ratingFailure = RATING_REFUSED
        publishTrack(1)
        recreateAt("w916dp-h412dp-land")

        compose.onNodeWithText("Dock mode").performClick()
        compose.waitForIdle()
        compose.onNodeWithTag("dock-star").performClick()
        compose.waitForIdle()

        assertEquals(listOf(1L to 5), application.controls.ratingRequests)
        compose.onNodeWithTag("dock-star").assert(
            SemanticsMatcher.expectValue(SemanticsProperties.StateDescription, "Rating 2 of 5"),
        )
        compose.onNodeWithText(RATING_REFUSED).assertExists()

        compose.onNodeWithContentDescription("Exit dock mode").performClick()
        compose.waitForIdle()

        (1..5).forEach { star -> star(star).assertRating(2) }
        compose.onNodeWithTag("library-track-row-1").assertTextContains("2/5")
    }

    private fun star(star: Int): SemanticsNodeInteraction =
        compose.onNodeWithContentDescription("Rate $star of 5 stars")

    private fun SemanticsNodeInteraction.assertRating(rating: Int): SemanticsNodeInteraction =
        assert(
            SemanticsMatcher.expectValue(
                SemanticsProperties.StateDescription,
                "Rated $rating of 5",
            ),
        )

    private fun publishTrack(trackId: Long) {
        application.service.publish(m9bSnapshot(trackId))
        shadowOf(Looper.getMainLooper()).idle()
        compose.waitForIdle()
    }

    private fun recreateAt(qualifiers: String) {
        RuntimeEnvironment.setQualifiers(qualifiers)
        compose.activityRule.scenario.recreate()
        shadowOf(Looper.getMainLooper()).idle()
        application.service.republish()
        shadowOf(Looper.getMainLooper()).idle()
        compose.waitForIdle()
    }
}

private const val RATING_REFUSED = "Could not save rating: track is missing."
