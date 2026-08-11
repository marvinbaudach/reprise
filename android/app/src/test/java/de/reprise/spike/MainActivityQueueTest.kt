package de.reprise.spike

import android.os.Looper
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.unit.dp
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

/** Queue-tab claims exercise MainActivity, its service bind, and the real page. */
@RunWith(RobolectricTestRunner::class)
@Config(
    sdk = [36],
    qualifiers = "w412dp-h916dp-port",
    application = ConfigurationTestApplication::class,
)
class MainActivityQueueTest {
    @get:Rule
    val compose = createAndroidComposeRule<MainActivity>()

    private val application: ConfigurationTestApplication
        get() = RuntimeEnvironment.getApplication() as ConfigurationTestApplication

    @After
    fun releaseTheService() {
        application.releaseService()
    }

    @Test
    fun queuePageListsOnlyWhatFollowsThePlayingTrack() {
        openQueue()

        compose.onNodeWithTag("now-playing-queue").assertIsDisplayed()
        compose.onNodeWithText("Upcoming One").assertIsDisplayed()
        compose.onNodeWithText("Upcoming Two").assertIsDisplayed()
        assertEquals(listOf(LibraryWindowRange(0, 200)), application.controls.loadUpcomingRequests)
        compose.onNode(
            hasText("Rotation Song 1") and hasAnyAncestor(hasTestTag("now-playing-queue")),
            useUnmergedTree = true,
        ).assertDoesNotExist()
    }

    @Test
    fun tappingPromotesOnlyThatRowAndCarriesItsIdentity() {
        openQueue()

        compose.onNodeWithTag("queue-track-row-502").performClick()
        compose.waitForIdle()

        assertEquals(listOf(1 to 502L), application.controls.playUpcomingRequests)
        assertEquals(4, application.currentQueue.size)
        assertEquals(1, application.currentQueueIndex)
        assertEquals(listOf(1L, 502L, 501L, 503L), application.currentQueue.map { it.id })
        compose.onNodeWithText("Upcoming One").assertIsDisplayed()
        compose.onNodeWithText("Upcoming Three").assertIsDisplayed()
        compose.onNode(
            hasText("Upcoming Two") and hasAnyAncestor(hasTestTag("now-playing-queue")),
            useUnmergedTree = true,
        ).assertDoesNotExist()
    }

    @Test
    fun dragHandleMovesDownToTheDroppedPosition() {
        openQueue()

        dragHandle(trackId = 501, rowHeights = 2f)
        compose.waitForIdle()
        assertEquals(Triple(0, 501L, 2), application.controls.moveUpcomingRequests.last())
        assertEquals(listOf(502L, 503L, 501L), upcomingIds())
    }

    @Test
    fun dragHandleMovesUpToTheDroppedPosition() {
        openQueue()

        dragHandle(trackId = 503, rowHeights = -2f)
        compose.waitForIdle()
        assertEquals(Triple(2, 503L, 0), application.controls.moveUpcomingRequests.last())
        assertEquals(listOf(503L, 501L, 502L), upcomingIds())
    }

    @Test
    fun fullWidthSwipeRemovesButShortFlickDoesNot() {
        openQueue()

        swipeRow(trackId = 501, fraction = 0.3f)
        compose.waitForIdle()
        assertEquals(emptyList<Pair<Int, Long>>(), application.controls.removeUpcomingRequests)
        assertEquals(listOf(501L, 502L, 503L), upcomingIds())

        swipeRow(trackId = 501, fraction = 0.9f)
        compose.waitForIdle()
        assertEquals(listOf(0 to 501L), application.controls.removeUpcomingRequests)
        assertEquals(listOf(502L, 503L), upcomingIds())
    }

    @Test
    fun staleFalseReloadsTruthInsteadOfLeavingTheCapturedRows() {
        openQueue()
        application.removeUpcomingBehindScreen(trackId = 501)

        swipeRow(trackId = 502, fraction = 0.9f)
        compose.waitForIdle()

        assertEquals(listOf(1 to 502L), application.controls.removeUpcomingRequests)
        compose.onNodeWithText("Upcoming One").assertDoesNotExist()
        compose.onNodeWithText("Upcoming Two").assertIsDisplayed()
        compose.onNodeWithText("Upcoming Three").assertIsDisplayed()
    }

    @Test
    fun exhaustedQueueExplainsItselfAndSurvivesRecreate() {
        application.replaceQueue(queueFixture().take(1))
        publishPlayingTrack()
        compose.onNodeWithTag("library-destination-QUEUE").performClick()
        compose.onNodeWithText("The queue is exhausted.").assertIsDisplayed()

        compose.activityRule.scenario.recreate()
        shadowOf(Looper.getMainLooper()).idle()
        application.service.republish()
        shadowOf(Looper.getMainLooper()).idle()
        compose.waitForIdle()
        compose.onNodeWithText("The queue is exhausted.").assertIsDisplayed()
    }

    @Test
    fun queueTabIsTheOnlyQueueRouteAndNowPlayingHasNoToggle() {
        openQueue()
        assertEquals(BrowseTab.TITLES, application.rememberedDestination)
        assertEquals(emptyList<BrowseTab>(), application.rememberedDestinationWrites)
        compose.onNodeWithContentDescription("Show queue").assertDoesNotExist()

        compose.onNodeWithTag("library-mini-player").performClick()
        compose.onNodeWithContentDescription("Collapse Now Playing").assertIsDisplayed()
        compose.onNodeWithContentDescription("Show queue").assertDoesNotExist()
    }

    @Test
    fun stackedAndWideShortLibraryLayoutsBothDrawTheQueueTab() {
        openQueue()
        compose.onNodeWithTag("now-playing-queue").assertIsDisplayed()

        RuntimeEnvironment.setQualifiers("w916dp-h412dp-land")
        compose.activityRule.scenario.recreate()
        shadowOf(Looper.getMainLooper()).idle()
        application.service.republish()
        shadowOf(Looper.getMainLooper()).idle()
        compose.waitForIdle()

        compose.onNodeWithTag("now-playing-queue").assertIsDisplayed()
        compose.onNodeWithTag("library-navigation-rail").assertIsDisplayed()
        compose.onNodeWithContentDescription("Show queue").assertDoesNotExist()
        val firstRowTop = compose.onNodeWithTag("queue-track-row-501")
            .fetchSemanticsNode().boundsInRoot.top
        val secondRowTop = compose.onNodeWithTag("queue-track-row-502")
            .fetchSemanticsNode().boundsInRoot.top
        assertEquals(firstRowTop, secondRowTop, 0.5f)
    }

    private fun openQueue() {
        application.replaceQueue(queueFixture())
        publishPlayingTrack()
        compose.onNodeWithTag("library-destination-QUEUE").performClick()
        compose.onNodeWithTag("now-playing-queue").assertIsDisplayed()
    }

    private fun publishPlayingTrack() {
        application.service.publish(m9bSnapshot(trackId = 1))
        shadowOf(Looper.getMainLooper()).idle()
        compose.waitForIdle()
    }

    private fun dragHandle(trackId: Long, rowHeights: Float) {
        compose.onNodeWithTag(
            "queue-drag-handle-$trackId",
            useUnmergedTree = true,
        ).performTouchInput {
            down(center)
            moveTo(Offset(center.x, center.y + 72.dp.toPx() * rowHeights))
            up()
        }
    }

    private fun swipeRow(trackId: Long, fraction: Float) {
        compose.onNodeWithTag("queue-track-row-$trackId").performTouchInput {
            down(Offset(width * 0.95f, centerY))
            moveTo(Offset(width * (0.95f - fraction), centerY))
            up()
        }
    }

    private fun upcomingIds(): List<Long> {
        val current = checkNotNull(application.currentQueueIndex)
        return application.currentQueue.drop(current + 1).map(LibraryTrack::id)
    }

    private fun queueFixture() = listOf(
        configurationTestTrack(1, "Rotation Song 1"),
        configurationTestTrack(501, "Upcoming One"),
        configurationTestTrack(502, "Upcoming Two"),
        configurationTestTrack(503, "Upcoming Three"),
    )
}
