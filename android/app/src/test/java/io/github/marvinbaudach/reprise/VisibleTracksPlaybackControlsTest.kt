package io.github.marvinbaudach.reprise

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class VisibleTracksPlaybackControlsTest {
    private val messages = RecordingDeletionMessages()
    private val deletions = PendingDeletions(
        offers = UndoOffers { _, _ -> },
        messages = messages,
        currentTrackId = { null },
        latestRefreshTicket = { 0 },
        scopeFactory = { CoroutineScope(SupervisorJob() + Dispatchers.Unconfined) },
    )
    private val real = RecordingPlayControls(FakeQueueControls(listOf(1, 2, 3, 4, 5)))
    private val visible = VisibleTracksPlaybackControls(real, deletions)

    @Test
    fun whatIsAboutToBeDeletedIsNotPlayedAndTheStartFollowsTheTrack() {
        deletions.begin(listOf(2, 4), real)

        // Index 3 is track 4, which is going: play starts on the next one left.
        visible.playTrackIds(listOf(1, 2, 3, 4, 5), 3)

        assertEquals(listOf(listOf(1L, 3L, 5L) to 2), real.played)
    }

    @Test
    fun aListWithNothingHiddenIsPlayedAsAsked() {
        visible.playTrackIds(listOf(1, 2, 3), 1)

        assertEquals(listOf(listOf(1L, 2L, 3L) to 1), real.played)
    }

    @Test
    fun aListThatIsAllHiddenPlaysNothingAndSaysWhy() {
        deletions.begin(listOf(1, 2), real)

        visible.playTrackIds(listOf(1, 2), 0)

        assertEquals(emptyList<Pair<List<Long>, Int>>(), real.played)
        assertEquals(listOf(EVERYTHING_TAPPED_IS_BEING_DELETED), messages.lines)
    }

    @Test
    fun everythingElseReachesTheRealTransport() {
        var rows = -1L
        visible.loadUpcomingTracks(LibraryWindowRange(0, 10)) { rows = it.getOrThrow().total }

        assertEquals(5L, rows)
        visible.deleteTracks(listOf(3)) {}
        assertEquals(listOf(listOf(3L)), real.deleted)
    }
}

/** Records plays; every other call goes to [queue]. */
private class RecordingPlayControls(private val queue: FakeQueueControls) :
    PlaybackControls by queue {
    val played = mutableListOf<Pair<List<Long>, Int>>()
    val deleted get() = queue.deleted

    override fun playTrackIds(trackIds: List<Long>, startIndex: Int) {
        played += trackIds to startIndex
    }
}
