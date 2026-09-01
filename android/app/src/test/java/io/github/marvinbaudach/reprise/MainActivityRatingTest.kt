package io.github.marvinbaudach.reprise

import android.os.Looper
import androidx.compose.ui.test.assertContentDescriptionEquals
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsNotSelected
import androidx.compose.ui.test.assertIsSelected
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
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
    fun libraryHeartWritesFiveThenZeroAndAFreshActivityReadsAnotherTracksStoredRating() {
        libraryHeart("Add to favourites").performClick()
        compose.waitForIdle()

        assertEquals(listOf(1L to 5), application.controls.ratingRequests)
        assertEquals(5, application.trackRatings[1L])
        assertEquals(emptyList<LibraryTrack>(), application.currentQueue)

        libraryHeart("Remove from favourites").performClick()
        compose.waitForIdle()

        assertEquals(listOf(1L to 5, 1L to 0), application.controls.ratingRequests)
        assertEquals(0, application.trackRatings[1L])
        libraryHeart("Add to favourites").assertIsDisplayed()

        // Track 2, not track 1: the view model's optimistic rating cache survives
        // recreate(), so track 1 would show the heart even if nothing were read back.
        application.trackRatings[2L] = 5
        application.catalogSize += 1
        recreate()
        compose.onNode(
            hasTestTag(TRACK_HEART_TAG) and
                hasContentDescription("Remove from favourites") and
                hasAnyAncestor(hasTestTag("library-track-row-2")) and
                hasAnyAncestor(hasTestTag("library-page-TITLES")),
        ).assertIsDisplayed()
    }

    @Test
    fun sheetDockAndLibraryReadOneHeartWithoutRestoreMemory() {
        publishTrack(1)
        compose.onNodeWithTag("library-mini-player").performClick()
        compose.onNodeWithTag("now-playing-heart").performClick()
        compose.waitForIdle()

        assertEquals(listOf(1L to 5), application.controls.ratingRequests)
        compose.activity.onBackPressedDispatcher.onBackPressed()
        compose.waitForIdle()
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
    fun sheetHeaderKeepsSleepAndHeartWithoutAQueueActionAndTransportStaysAtTheBottom() {
        publishTrack(1)
        compose.onNodeWithTag("library-mini-player").performClick()

        val actionRow = hasTestTag("now-playing-actions")
        // Sleep, heart and the context menu — the collapse button gave way to
        // the downward swipe, and the fullscreen visualizer is retired.
        compose.onAllNodes(hasClickAction() and hasAnyAncestor(actionRow))
            .assertCountEquals(3)
        val sleep = compose.onNode(
            hasContentDescription("Set sleep timer") and hasAnyAncestor(actionRow),
        )
        val heart = compose.onNode(
            hasTestTag("now-playing-heart") and hasAnyAncestor(actionRow),
        ).assertContentDescriptionEquals("Add to favourites")
        val overflow = compose.onNode(
            hasTestTag("now-playing-overflow") and hasAnyAncestor(actionRow),
        )

        val sleepBounds = sleep.getUnclippedBoundsInRoot()
        val heartBounds = heart.getUnclippedBoundsInRoot()
        val overflowBounds = overflow.getUnclippedBoundsInRoot()
        assertEquals(sleepBounds.top.value, heartBounds.top.value, 0.5f)
        assertTrue(sleepBounds.left < heartBounds.left)
        assertTrue(heartBounds.left < overflowBounds.left)
        compose.onNodeWithContentDescription("Collapse Now Playing").assertDoesNotExist()
        compose.onNodeWithContentDescription("Open fullscreen visualizer").assertDoesNotExist()

        val transportRow = compose.onNodeWithTag("now-playing-transport")
        val transportBottom = transportRow.getUnclippedBoundsInRoot().bottom
        val contentBottom = compose.onNodeWithTag("now-playing-content")
            .getUnclippedBoundsInRoot()
            .bottom
        assertTrue(
            "transport left too much inactive space below it: $transportBottom..$contentBottom",
            contentBottom - transportBottom <= 24.dp,
        )

        // Nothing may be placed under the transport row. The scene draws in
        // layers now, so the full-bleed background and the row's own padded
        // frame reach past it; both begin above the row. Anything that begins
        // below it and still renders lower is content that escaped downwards.
        val transport = transportRow.fetchSemanticsNode().boundsInRoot
        val laterContent = compose.onAllNodes(
            hasAnyAncestor(hasTestTag("now-playing-content")),
            useUnmergedTree = true,
        ).fetchSemanticsNodes().filter { node ->
            node.boundsInRoot.top > transport.top + 0.5f &&
                node.boundsInRoot.bottom > transport.bottom + 0.5f
        }
        assertTrue(
            "content continued below the transport row: ${laterContent.map { it.boundsInRoot }}",
            laterContent.isEmpty(),
        )

        heart.performClick()
        compose.waitForIdle()
        assertEquals(listOf(1L to 5), application.controls.ratingRequests)
        compose.onNodeWithTag("now-playing-heart")
            .assertContentDescriptionEquals("Remove from favourites")
        compose.onNodeWithText("0 plays").assertDoesNotExist()
    }

    @Test
    fun shuffle_mode_is_visibly_inactive_then_active_from_the_playback_snapshot() {
        publishTrack(1)
        compose.onNodeWithTag("library-mini-player").performClick()
        compose.onNodeWithTag("now-playing-shuffle").assertIsNotSelected()

        application.service.publish(m9bSnapshot(1).copy(shuffled = true))
        shadowOf(Looper.getMainLooper()).idle()
        compose.waitForIdle()

        compose.onNodeWithTag("now-playing-shuffle").assertIsSelected()
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
