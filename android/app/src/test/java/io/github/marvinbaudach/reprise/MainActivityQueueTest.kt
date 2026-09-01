package io.github.marvinbaudach.reprise

import android.os.Looper
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsSelected
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.longClick
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollToIndex
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.swipeRight
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

    /**
     * The three things the drop is built around, measured on the real screen.
     *
     * A reorder that recomputed the list while the finger was down would show
     * none of them: the row would jump rather than lift, the rows around it
     * would re-key rather than step aside, and the edit would land in the
     * middle of the animation. So the claims are the lift, the parting, and
     * the fact that nothing is written until the row has arrived.
     *
     * The middle row is the one under test because it is the only one with a
     * neighbour on either side and no list edge cropping what the lift adds.
     */
    @Test
    fun theRowLiftsItsNeighboursPartAndTheEditWaitsForTheDrop() {
        openQueue()
        compose.mainClock.autoAdvance = false
        try {
            val restingHeight = rowBounds(502).height
            val restingThirdTop = rowBounds(503).top

            handle(502).performTouchInput { down(center) }
            compose.waitForIdle()
            // Generously past the envelope: with the clock paused the first
            // frame of an animation is spent starting it, so "one duration"
            // would measure the curve mid-flight rather than at rest.
            compose.mainClock.advanceTimeBy(QUEUE_DRAG_LIFT_MS * 4L)

            // Picked up, not merely selected: the row stands 2 % proud of the
            // list it is being carried over.
            assertEquals(
                restingHeight * QUEUE_DRAG_LIFT_SCALE,
                rowBounds(502).height,
                0.3f,
            )

            handle(502).performTouchInput { moveBy(Offset(0f, 72.dp.toPx())) }
            // A paused clock does not drain the recomposition the pointer
            // event just queued; advancing time alone would measure the frame
            // before the neighbours were told anything.
            compose.waitForIdle()
            compose.mainClock.advanceTimeBy(QUEUE_DRAG_NEIGHBOUR_MS * 3L)

            // The row below has stepped up into the gap rather than the list
            // having been rebuilt underneath it.
            assertEquals(restingThirdTop - restingHeight, rowBounds(503).top, 1f)
            assertEquals(
                "nothing may be written while the finger is still down",
                emptyList<Triple<Int, Long, Int>>(),
                application.controls.moveUpcomingRequests,
            )

            handle(502).performTouchInput { up() }
            compose.waitForIdle()
            compose.mainClock.advanceTimeBy(QUEUE_DRAG_DROP_MS - FRAME_MS * 4)
            assertEquals(
                "the edit belongs after the row has landed, not on lift-off",
                emptyList<Triple<Int, Long, Int>>(),
                application.controls.moveUpcomingRequests,
            )

            compose.mainClock.advanceTimeBy(QUEUE_DRAG_DROP_MS.toLong())
            assertEquals(
                listOf(Triple(1, 502L, 2)),
                application.controls.moveUpcomingRequests,
            )
        } finally {
            compose.mainClock.autoAdvance = true
        }
        compose.waitForIdle()
        assertEquals(listOf(501L, 503L, 502L), upcomingIds())
    }

    /**
     * The queue is longer than the screen, so a reorder that could only reach
     * what is already visible would not be a reorder at all. Holding the row
     * against the top edge walks the list under it, and every pixel the list
     * travels counts towards the drop the same way a pixel of finger travel
     * does — otherwise the row would land wherever the *finger* pointed while
     * the slots slid past underneath.
     */
    @Test
    fun holdingTheRowAtTheTopEdgeWalksTheListUnderIt() {
        application.replaceQueue(longQueueFixture())
        publishPlayingTrack()
        compose.onNodeWithTag("library-destination-QUEUE").performClick()
        compose.onNodeWithTag("now-playing-queue").assertIsDisplayed()
        compose.onNodeWithTag("now-playing-queue").performScrollToIndex(LAST_LONG_SLOT)
        compose.waitForIdle()
        compose.onNodeWithText("Queued 0").assertDoesNotExist()

        compose.mainClock.autoAdvance = false
        try {
            handle(QUEUE_TAIL_TRACK_ID).performTouchInput {
                down(center)
                // Far above the list's own top edge, and left there: the
                // finger stops moving, the list does not.
                moveBy(Offset(0f, -600.dp.toPx()))
            }
            compose.waitForIdle()
            // Frame by frame on purpose: the loop takes exactly one step per
            // frame, and a bulk time jump is not the same as the frames it
            // would have contained.
            repeat(AUTOSCROLL_HOLD_FRAMES) { compose.mainClock.advanceTimeByFrame() }
            handle(QUEUE_TAIL_TRACK_ID).performTouchInput { up() }
            compose.waitForIdle()
            compose.mainClock.advanceTimeBy(QUEUE_DRAG_DROP_MS * 2L)
        } finally {
            compose.mainClock.autoAdvance = true
        }
        compose.waitForIdle()

        // The finger itself is worth about eight rows of the twenty-nine; the
        // row landed at the head of the queue, which only the list's own
        // travel can account for.
        assertEquals(
            Triple(LAST_LONG_SLOT, QUEUE_TAIL_TRACK_ID, 0),
            application.controls.moveUpcomingRequests.last(),
        )
        compose.onNodeWithText("Queued 0").assertIsDisplayed()
    }

    @Test
    fun aDragThatEndsWhereItStartedWritesNothing() {
        openQueue()

        handle(501).performTouchInput {
            down(center)
            moveBy(Offset(0f, 72.dp.toPx() * 0.3f))
            up()
        }
        compose.waitForIdle()

        assertEquals(emptyList<Triple<Int, Long, Int>>(), application.controls.moveUpcomingRequests)
        assertEquals(listOf(501L, 502L, 503L), upcomingIds())
    }

    @Test
    fun fullWidthHorizontalSwipeLeavesTheQueueUntouched() {
        openQueue()

        swipeRow(trackId = 501, fraction = 0.9f)
        compose.waitForIdle()
        assertEquals(emptyList<Pair<Int, Long>>(), application.controls.removeUpcomingRequests)
        assertEquals(listOf(501L, 502L, 503L), upcomingIds())
    }

    @Test
    fun staleFalseReloadsTruthInsteadOfLeavingTheCapturedRows() {
        openQueue()
        application.removeUpcomingBehindScreen(trackId = 501)

        compose.onNodeWithTag("queue-track-row-502").performTouchInput { longClick() }
        compose.onNodeWithText("Remove from queue").performClick()
        compose.waitForIdle()

        assertEquals(listOf(1 to 502L), application.controls.removeUpcomingRequests)
        compose.onNodeWithText("Upcoming One").assertDoesNotExist()
        compose.onNodeWithText("Upcoming Two").assertIsDisplayed()
        compose.onNodeWithText("Upcoming Three").assertIsDisplayed()
    }

    @Test
    fun contextMenuRemainsTheQueueRemovalPath() {
        openQueue()

        compose.onNodeWithTag("queue-track-row-501").performTouchInput { longClick() }
        compose.onNodeWithText("Remove from queue").performClick()
        compose.waitForIdle()

        assertEquals(listOf(0 to 501L), application.controls.removeUpcomingRequests)
        assertEquals(listOf(502L, 503L), upcomingIds())
    }

    @Test
    fun horizontalSwipeStartedOnAQueueRowChangesTheSelectedTab() {
        openQueue()

        compose.onNodeWithTag("queue-track-row-501").performTouchInput { swipeRight() }
        compose.waitForIdle()

        compose.onNodeWithTag("library-destination-ARTISTS").assertIsSelected()
        assertEquals(emptyList<Pair<Int, Long>>(), application.controls.removeUpcomingRequests)
        assertEquals(listOf(501L, 502L, 503L), upcomingIds())
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
        // Now Playing is dismissed by swiping it down rather than by a button,
        // so the sheet's own content is what says it opened.
        compose.onNodeWithTag("now-playing-content").assertIsDisplayed()
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
        handle(trackId).performTouchInput {
            down(center)
            moveBy(Offset(0f, 72.dp.toPx() * rowHeights))
            up()
        }
    }

    private fun handle(trackId: Long) =
        compose.onNodeWithTag("queue-drag-handle-$trackId", useUnmergedTree = true)

    /**
     * Where the row is drawn, in pixels, transforms included — which is the
     * whole point: the lift and the parting are a `graphicsLayer`, never a
     * re-layout, so a measurement that ignored the layer would report the
     * gesture as having done nothing at all.
     */
    private fun rowBounds(trackId: Long) =
        compose.onNodeWithTag("queue-track-row-$trackId").fetchSemanticsNode().boundsInRoot

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

    private companion object {
        /** One frame at 60 Hz, rounded the way the test clock counts. */
        const val FRAME_MS = 16L

        /** Enough frames to walk thirty rows of 72 dp back to the top. */
        const val AUTOSCROLL_HOLD_FRAMES = 260

        const val LONG_QUEUE_SIZE = 30
        const val LAST_LONG_SLOT = LONG_QUEUE_SIZE - 1
        const val QUEUE_TAIL_TRACK_ID = 600L + LAST_LONG_SLOT
    }

    private fun longQueueFixture() = listOf(configurationTestTrack(1, "Rotation Song 1")) +
        (0 until LONG_QUEUE_SIZE).map { slot ->
            configurationTestTrack(600L + slot, "Queued $slot")
        }

    private fun queueFixture() = listOf(
        configurationTestTrack(1, "Rotation Song 1"),
        configurationTestTrack(501, "Upcoming One"),
        configurationTestTrack(502, "Upcoming Two"),
        configurationTestTrack(503, "Upcoming Three"),
    )
}
