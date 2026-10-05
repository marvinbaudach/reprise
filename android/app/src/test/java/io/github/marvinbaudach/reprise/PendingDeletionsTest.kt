package io.github.marvinbaudach.reprise

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.reprise_android_ffi.AndroidTrashFailure
import uniffi.reprise_android_ffi.AndroidTrashReport

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class PendingDeletionsTest {
    private val timers = ManualTimers()
    private val messages = RecordingDeletionMessages()
    private var playing: Long? = null
    private var refreshTicket = 0
    private val deletions = PendingDeletions(
        offers = UndoOffers(timers::schedule),
        messages = messages,
        currentTrackId = { playing },
        latestRefreshTicket = { refreshTicket },
        scopeFactory = { CoroutineScope(SupervisorJob() + Dispatchers.Unconfined) },
    )

    @Test
    fun beginningHidesTheTracksAndTakesThemOutOfTheQueueAtOnce() {
        val queue = FakeQueueControls(listOf(10, 11, 12, 13, 14))

        deletions.begin(listOf(11, 13), queue)

        assertTrue(deletions.isHidden(11))
        assertTrue(deletions.isHidden(13))
        assertFalse(deletions.isHidden(12))
        assertEquals(listOf(10L, 12L, 14L), queue.upcoming)
        assertEquals("2 tracks will be deleted", deletions.offers.current?.message)
        assertEquals(emptyList<List<Long>>(), queue.deleted)
    }

    @Test
    fun undoRestoresTheRowsAndTheQueueExactly() {
        val queue = FakeQueueControls(listOf(10, 11, 12, 13, 14, 15))

        deletions.begin(listOf(11, 13, 15), queue)
        deletions.offers.undo(checkNotNull(deletions.offers.current).token)

        assertEquals(listOf(10L, 11L, 12L, 13L, 14L, 15L), queue.upcoming)
        assertFalse(deletions.isHidden(11))
        assertNull(deletions.offers.current)
        timers.fireAll()
        assertEquals(emptyList<List<Long>>(), queue.deleted)
    }

    @Test
    fun whenTheWindowPassesTheFilesAreDeletedOnce() {
        val queue = FakeQueueControls(listOf(10, 11, 12))
        refreshTicket = 3

        deletions.begin(listOf(11), queue)
        assertEquals(UNDO_WINDOW_MS, timers.delays.single())
        timers.fireAll()
        timers.fireAll()

        assertEquals(listOf(listOf(11L)), queue.deleted)
        assertNull(deletions.offers.current)
        assertEquals(listOf("Deleting 1 track…", "1 track deleted"), messages.lines)
        // Confirmed gone, but kept hidden until the library re-read has landed.
        assertTrue(deletions.isHidden(11))
        deletions.libraryRefreshed(2)
        assertTrue(deletions.isHidden(11))
        deletions.libraryRefreshed(3)
        assertFalse(deletions.isHidden(11))
    }

    @Test
    fun aSecondDeleteCommitsTheFirstImmediately() {
        val queue = FakeQueueControls(listOf(10, 11, 12, 13))

        deletions.begin(listOf(10), queue)
        deletions.begin(listOf(12, 13), queue)

        assertEquals(listOf(listOf(10L)), queue.deleted)
        // The first window's timer fires later and finds nothing to do.
        timers.fire(0)
        assertEquals(listOf(listOf(10L)), queue.deleted)
        timers.fire(1)
        assertEquals(listOf(listOf(10L), listOf(12L, 13L)), queue.deleted)
    }

    @Test
    fun aClearedScreenDeletesNothingEvenIfItsTimerFiresLater() {
        val queue = FakeQueueControls(listOf(10, 11, 12))

        deletions.begin(listOf(11), queue)
        // The view model is cleared inside the window; the timer still fires.
        deletions.close()
        timers.fireAll()

        assertEquals(emptyList<List<Long>>(), queue.deleted)
    }

    @Test
    fun aPlayingTrackSkipsOnAfterTheQueueWasEditedAndItsRowsReturnAsNext() {
        val queue = FakeQueueControls(listOf(11, 12, 13))
        playing = 5

        deletions.begin(listOf(5, 12), queue)
        assertEquals(listOf(11L, 13L), queue.upcoming)
        assertEquals(1, queue.skips)

        deletions.offers.undo(checkNotNull(deletions.offers.current).token)
        // The playing track is behind us now; positions mean nothing, so the
        // row that was queued comes back as the next one.
        assertEquals(listOf(12L, 11L, 13L), queue.upcoming)
    }

    @Test
    fun aQueueThatChangedShapeTakesUndoneRowsBackAsNext() {
        val queue = FakeQueueControls(listOf(10, 11, 12))

        deletions.begin(listOf(11), queue)
        queue.upcoming.add(99)
        deletions.offers.undo(checkNotNull(deletions.offers.current).token)

        assertEquals(listOf(11L, 10L, 12L, 99L), queue.upcoming)
    }

    @Test
    fun whatFailedToDeleteComesBackAtOnce() {
        val queue = FakeQueueControls(
            listOf(10, 11, 12),
            outcome = Result.success(
                AndroidTrashReport(
                    removedIds = listOf(10),
                    failures = listOf(AndroidTrashFailure(11, "content://x", "denied")),
                ),
            ),
        )

        deletions.begin(listOf(10, 11), queue)
        timers.fireAll()

        assertTrue(deletions.isHidden(10))
        assertFalse(deletions.isHidden(11))
        assertEquals("1 of 2 could not be deleted", messages.lines.last())
    }

    @Test
    fun aFailedDeleteBringsEveryRowBack() {
        val queue = FakeQueueControls(
            listOf(10, 11),
            outcome = Result.failure(IllegalStateException("disk full")),
        )

        deletions.begin(listOf(10, 11), queue)
        timers.fireAll()

        assertFalse(deletions.isHidden(10))
        assertEquals(
            "Could not delete tracks: disk full",
            messages.lines.last(),
        )
    }

    @Test
    fun aDeleteWhoseWindowPassesWithoutAnActivityWaitsForTheNextOne() {
        val first = FakeQueueControls(listOf(10, 11))
        val second = FakeQueueControls(listOf(10, 11))

        deletions.begin(listOf(10), first)
        deletions.unbind(first)
        timers.fireAll()
        assertEquals(emptyList<List<Long>>(), first.deleted)

        deletions.bind(second)

        assertEquals(listOf(listOf(10L)), second.deleted)
    }

    @Test
    fun aTapDoesNotPlayWhatIsAboutToBeDeleted() {
        val tracks = (1L..5L).map { configurationTestTrack(it, "T$it") }
        deletions.begin(listOf(2, 4), FakeQueueControls(emptyList()))

        val selection = deletions.visibleSelection(PlaybackSelection(tracks, 4))

        assertEquals(listOf(1L, 3L, 5L), selection?.tracks?.map { it.id })
        assertEquals(2, selection?.startIndex)
        assertNull(deletions.visibleSelection(PlaybackSelection(tracks, 1)))
    }

    @Test
    fun removingAQueueRowOffersAnUndoThatPutsItBackWhereItWas() {
        val queue = FakeQueueControls(listOf(10, 11, 12, 13))
        var refreshed = 0

        deletions.removeFromQueueWithUndo(
            position = 1,
            trackId = 11,
            playback = queue,
            remove = { queue.removeUpcomingTrack(1, 11) {} },
            refresh = { refreshed += 1 },
        )

        assertEquals(QUEUE_REMOVED_MESSAGE, deletions.offers.current?.message)
        assertEquals(listOf(10L, 12L, 13L), queue.upcoming)
        deletions.offers.undo(checkNotNull(deletions.offers.current).token)

        assertEquals(listOf(10L, 11L, 12L, 13L), queue.upcoming)
        assertEquals(1, refreshed)
    }

    @Test
    fun aQueueRowThatCouldNotBeRemovedOffersNoUndo() {
        val queue = FakeQueueControls(listOf(10, 11, 12))

        deletions.removeFromQueueWithUndo(
            position = 1,
            trackId = 99,
            playback = queue,
            remove = { queue.removeUpcomingTrack(1, 99) {} },
            refresh = {},
        )

        assertNull(deletions.offers.current)
        assertEquals(listOf(10L, 11L, 12L), queue.upcoming)
    }

    @Test
    fun aQueueUndoAfterTheQueueChangedShapeAppendsTheRowNext() {
        val queue = FakeQueueControls(listOf(10, 11, 12))

        deletions.removeFromQueueWithUndo(
            position = 2,
            trackId = 12,
            playback = queue,
            remove = { queue.removeUpcomingTrack(2, 12) {} },
            refresh = {},
        )
        queue.upcoming.removeAt(0)
        deletions.offers.undo(checkNotNull(deletions.offers.current).token)

        assertEquals(listOf(12L, 11L), queue.upcoming)
    }

    @Test
    fun aQueueRemovalWaitsForAPendingDeletesWindowInsteadOfEndingIt() {
        val queue = FakeQueueControls(listOf(10, 11, 12))

        deletions.begin(listOf(10), queue)
        deletions.removeFromQueueWithUndo(
            position = 1,
            trackId = 12,
            playback = queue,
            remove = { queue.removeUpcomingTrack(1, 12) {} },
            refresh = {},
        )

        // The delete keeps the slot, its undo and its files.
        assertEquals("1 track will be deleted", deletions.offers.current?.message)
        assertEquals(emptyList<List<Long>>(), queue.deleted)

        // Its own window passing commits it once, however often the timer fires.
        timers.fire(0)
        timers.fire(0)
        assertEquals(listOf(listOf(10L)), queue.deleted)

        // Only now does the queue's undo come up, with a window of its own.
        assertEquals(QUEUE_REMOVED_MESSAGE, deletions.offers.current?.message)
        assertEquals(listOf(UNDO_WINDOW_MS, UNDO_WINDOW_MS), timers.delays)
        deletions.offers.undo(checkNotNull(deletions.offers.current).token)
        assertEquals(listOf(11L, 12L), queue.upcoming)
    }

    @Test
    fun aDeletesUndoBringsTheWaitingQueueOfferUp() {
        val queue = FakeQueueControls(listOf(10, 11, 12))

        deletions.begin(listOf(10), queue)
        deletions.removeFromQueueWithUndo(
            position = 1,
            trackId = 12,
            playback = queue,
            remove = { queue.removeUpcomingTrack(1, 12) {} },
            refresh = {},
        )
        deletions.offers.undo(checkNotNull(deletions.offers.current).token)

        assertEquals(QUEUE_REMOVED_MESSAGE, deletions.offers.current?.message)
        assertEquals(emptyList<List<Long>>(), queue.deleted)
    }

    @Test
    fun aNewDeleteDropsTheQueueOfferThatWasWaiting() {
        val queue = FakeQueueControls(listOf(10, 11, 12, 13))

        deletions.begin(listOf(10), queue)
        deletions.removeFromQueueWithUndo(
            position = 1,
            trackId = 12,
            playback = queue,
            remove = { queue.removeUpcomingTrack(1, 12) {} },
            refresh = {},
        )
        deletions.begin(listOf(13), queue)

        assertEquals(listOf(listOf(10L)), queue.deleted)
        assertEquals("1 track will be deleted", deletions.offers.current?.message)
        timers.fire(1)
        assertNull(deletions.offers.current)
    }

    @Test
    fun theOffersWindowIsTheOneTheListenersSettingsNeed() {
        deletions.undoWindowMs = { 15_000L }

        deletions.begin(listOf(10), FakeQueueControls(listOf(10)))

        assertEquals(15_000L, timers.delays.single())
    }
}
