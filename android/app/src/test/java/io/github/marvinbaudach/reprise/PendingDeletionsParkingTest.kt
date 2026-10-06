package io.github.marvinbaudach.reprise

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.reprise_android_ffi.TrashAction

/**
 * What a deferred delete does when the window it ends in cannot reach the
 * transport: in the background, mid-rotation, or while the service binds.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class PendingDeletionsParkingTest {
    private val timers = ManualTimers()
    private val messages = RecordingDeletionMessages()
    private val manual = ManualDispatcher()
    private var useManual = false
    private val deletions = PendingDeletions(
        offers = UndoOffers(timers::schedule),
        messages = messages,
        currentTrackId = { null },
        latestRefreshTicket = { 0 },
        scopeFactory = {
            CoroutineScope(SupervisorJob() + if (useManual) manual else Dispatchers.Unconfined)
        },
    )

    @Test
    fun anActivityWithoutAServiceAnswersWithTheTextThatIsRetried() {
        val controls = ActivityPlaybackControls(
            command = { _, _ -> },
            connectedService = { null },
            postToMain = { work -> work() },
            setFavouriteAction = { _, _, done -> done(null) },
            trashAction = object : TrashAction {
                override fun trash(uri: String): String? = null
            },
            playTrackIdsAction = { _, _ -> },
        )
        try {
            var answer: Throwable? = null
            controls.deleteTracks(listOf(1L)) { answer = it.exceptionOrNull() }

            assertEquals(PLAYBACK_STILL_CONNECTING, answer?.message)
        } finally {
            controls.shutdown()
        }
    }

    @Test
    fun aWindowThatExpiresInTheBackgroundDeletesWhenTheWindowReturns() {
        val queue = FakeQueueControls(listOf(10, 11, 12))

        deletions.begin(listOf(11), queue)
        deletions.setForeground(false)
        timers.fire(0)

        assertEquals(emptyList<List<Long>>(), queue.deleted)
        assertTrue(deletions.isHidden(11))
        assertEquals(emptyList<String>(), messages.lines)

        deletions.setForeground(true)

        assertEquals(listOf(listOf(11L)), queue.deleted)
        assertEquals(listOf("Deleting 1 track…", "1 track deleted"), messages.lines)
    }

    @Test
    fun aServiceThatIsStillConnectingIsAskedAgainWithoutAnErrorLine() {
        val queue = FakeQueueControls(listOf(10, 11, 12), connectFailures = 2)

        deletions.begin(listOf(11), queue)
        timers.fire(0)

        assertEquals(emptyList<String>(), messages.lines)
        assertTrue(deletions.isHidden(11))
        assertEquals(RECONNECT_RETRY_MS, timers.delays[1])

        timers.fire(1)
        assertEquals(emptyList<List<Long>>(), queue.deleted)
        timers.fire(2)

        assertEquals(listOf(listOf(11L)), queue.deleted)
        assertEquals(listOf("Deleting 1 track…", "1 track deleted"), messages.lines)
    }

    @Test
    fun aServiceThatNeverConnectsIsGivenUpOnUntilTheNextResume() {
        val queue = FakeQueueControls(listOf(10, 11), connectFailures = Int.MAX_VALUE)

        deletions.begin(listOf(11), queue)
        timers.fire(0)
        var fired = 1
        while (fired < timers.count) timers.fire(fired++)

        assertEquals(1 + RECONNECT_ATTEMPTS, timers.count)
        assertTrue(deletions.isHidden(11))
        assertEquals(emptyList<String>(), messages.lines)

        queue.connectFailures = 0
        deletions.setForeground(true)

        assertEquals(listOf(listOf(11L)), queue.deleted)
    }

    @Test
    fun aRotationBetweenTheWindowAndTheCommitParksUntilTheNextActivityBinds() {
        val first = FakeQueueControls(listOf(10, 11))
        val second = FakeQueueControls(listOf(10, 11))

        deletions.begin(listOf(11), first)
        deletions.unbind(first)
        timers.fire(0)
        deletions.bind(second)

        assertEquals(emptyList<List<Long>>(), first.deleted)
        assertEquals(listOf(listOf(11L)), second.deleted)
    }

    @Test
    fun anUndoPressedMidRotationRestoresTheQueueOnceTheNextActivityBinds() {
        val queue = FakeQueueControls(listOf(10, 11, 12))

        deletions.begin(listOf(11), queue)
        deletions.unbind(queue)
        deletions.offers.undo(checkNotNull(deletions.offers.current).token)

        assertFalse(deletions.isHidden(11))
        assertEquals(listOf(10L, 12L), queue.upcoming)

        deletions.bind(queue)

        assertEquals(listOf(10L, 11L, 12L), queue.upcoming)
        assertEquals(emptyList<List<Long>>(), queue.deleted)
    }

    @Test
    fun aQueueRowUndoneMidRotationComesBackOnceTheNextActivityBinds() {
        val queue = FakeQueueControls(listOf(10, 11, 12))
        var refreshed = 0
        deletions.bind(queue)
        deletions.removeFromQueueWithUndo(
            position = 1,
            trackId = 11,
            playback = queue,
            remove = { queue.removeUpcomingTrack(1, 11) {} },
            refresh = { refreshed += 1 },
        )

        deletions.unbind(queue)
        deletions.offers.undo(checkNotNull(deletions.offers.current).token)
        assertEquals(listOf(10L, 12L), queue.upcoming)
        deletions.bind(queue)

        assertEquals(listOf(10L, 11L, 12L), queue.upcoming)
        assertEquals(1, refreshed)
    }

    @Test
    fun closingWhileATransportIsBoundPutsTheQueueRowsBackAndDeletesNothing() {
        val queue = FakeQueueControls(listOf(10, 11, 12, 13))

        deletions.bind(queue)
        deletions.begin(listOf(11, 13), queue)
        assertEquals(listOf(10L, 12L), queue.upcoming)
        deletions.close()
        timers.fireAll()

        assertEquals(listOf(10L, 11L, 12L, 13L), queue.upcoming)
        assertEquals(emptyList<List<Long>>(), queue.deleted)
    }

    @Test
    fun closingWithNoTransportLeavesTheQueueToTheServiceAndDeletesNothing() {
        val queue = FakeQueueControls(listOf(10, 11, 12))

        deletions.begin(listOf(11), queue)
        deletions.unbind(queue)
        deletions.close()
        timers.fireAll()

        // Nobody can be asked: the rows stay out of the queue, the file stays.
        assertEquals(listOf(10L, 12L), queue.upcoming)
        assertEquals(emptyList<List<Long>>(), queue.deleted)
    }

    @Test
    fun closingAfterTheWindowParkedItsCommitPutsTheRowsBackToo() {
        val queue = FakeQueueControls(listOf(10, 11, 12))

        deletions.begin(listOf(11), queue)
        deletions.setForeground(false)
        timers.fire(0)
        deletions.close()

        assertEquals(listOf(10L, 11L, 12L), queue.upcoming)
        assertEquals(emptyList<List<Long>>(), queue.deleted)
    }

    @Test
    fun anUndoDuringAQueueEditThatIsStillRunningRestoresTheQueueExactly() {
        useManual = true
        val queue = FakeQueueControls(listOf(10, 11, 12, 13), holdRemovals = true)

        deletions.begin(listOf(11, 13), queue)
        manual.runAll()
        // The core has not answered the removals yet; the listener presses Undo.
        assertEquals(1, queue.heldRemovals.size)
        deletions.offers.undo(checkNotNull(deletions.offers.current).token)
        manual.runAll()
        assertEquals(emptyList<List<Long>>(), queue.deleted)

        queue.holdRemovals = false
        queue.releaseRemovals()
        manual.runAll()

        assertEquals(listOf(10L, 11L, 12L, 13L), queue.upcoming)
        assertFalse(deletions.isHidden(11))
        timers.fireAll()
        manual.runAll()
        assertEquals(emptyList<List<Long>>(), queue.deleted)
    }
}
