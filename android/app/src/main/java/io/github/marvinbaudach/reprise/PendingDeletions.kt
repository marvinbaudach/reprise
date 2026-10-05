package io.github.marvinbaudach.reprise

import android.util.Log
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch

private const val TAG = "PendingDeletions"

/**
 * One delete the listener may still take back. [sink] is where its outcome is
 * said: the screen's line, unless the surface that started it has a nearer one.
 */
private class PendingDeletion(val id: Long, val ids: List<Long>, val sink: DeletionMessages)

internal fun deletedMessage(count: Int): String =
    "$count ${if (count == 1) "track" else "tracks"} deleted"

internal const val QUEUE_REMOVED_MESSAGE = "Removed from queue"

internal const val DELETION_UNAVAILABLE = "Deleting is not available here."

/**
 * Deletes that are said, shown and undoable first, and carried out later.
 *
 * Choosing "Delete from device…" hides the tracks and takes them out of the
 * upcoming queue at once, and offers Undo for [UNDO_WINDOW_MS]. Only when the
 * window passes — or a newer offer takes the slot — does the file deletion run,
 * through the same `deleteTracks` path as before. A process that dies in the
 * window deletes nothing: the pending state lives in memory only, and
 * [close] discards it without committing.
 *
 * The same slot carries the queue's "Removed from queue · Undo"
 * ([removeFromQueueWithUndo]), because the host shows one line at a time.
 *
 * Rows are hidden at render time through [isHidden]; a window the lists page
 * through is never edited, since its paging offsets count its rows. A track
 * the delete confirmed gone stays hidden until the library re-read that follows
 * has landed ([libraryRefreshed]), so the row does not flash back in between.
 *
 * Main thread only.
 */
internal class PendingDeletions(
    val offers: UndoOffers,
    private val messages: DeletionMessages,
    private val currentTrackId: () -> Long?,
    private val latestRefreshTicket: () -> Int,
    scopeFactory: () -> CoroutineScope = {
        CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    },
) {
    private val scope by lazy(scopeFactory)
    private var hidden by mutableStateOf(emptySet<Long>())
    private val queueEdits = mutableMapOf<Long, Deferred<QueueRemoval>>()
    private val confirmedGone = mutableMapOf<Long, Int>()
    private val waitingForControls = mutableListOf<PendingDeletion>()
    private var controls: PlaybackControls? = null
    private var lastId = 0L

    fun isHidden(trackId: Long): Boolean = trackId in hidden

    fun withoutHidden(trackIds: List<Long>): List<Long> = trackIds.filterNot(::isHidden)

    /**
     * The selection a tap would play, without the tracks that are about to be
     * deleted. Null when the tapped row is one of them.
     */
    fun visibleSelection(selection: PlaybackSelection): PlaybackSelection? {
        val tapped = selection.tracks.getOrNull(selection.startIndex) ?: return selection
        if (isHidden(tapped.id)) return null
        return PlaybackSelection(
            tracks = selection.tracks.filterNot { isHidden(it.id) },
            startIndex = selection.tracks.take(selection.startIndex).count { !isHidden(it.id) },
        )
    }

    /** The transport a commit will use; the host keeps it current across rotation. */
    fun bind(playback: PlaybackControls) {
        controls = playback
        val due = waitingForControls.toList()
        waitingForControls.clear()
        due.forEach(::commit)
    }

    fun unbind(playback: PlaybackControls) {
        if (controls === playback) controls = null
    }

    /**
     * Hides [trackIds], takes them out of the upcoming queue, skips on when one
     * of them is playing, and offers the undo. A second call inside the window
     * commits the first delete: its undo is gone with the slot.
     */
    fun begin(
        trackIds: List<Long>,
        playback: PlaybackControls,
        sink: DeletionMessages = messages,
    ) {
        val ids = withoutHidden(trackIds.distinct())
        if (ids.isEmpty()) return
        controls = playback
        val pending = PendingDeletion(++lastId, ids, sink)
        hidden = hidden + ids
        val skipsCurrent = currentTrackId()?.let { it in ids } == true
        queueEdits[pending.id] = scope.async {
            val removal = playback.removeQueued(ids.toSet())
            if (!skipsCurrent) return@async removal
            // The queue's positions are relative to the playing track, so
            // skipping on has to wait until they were used; and a queue that
            // has moved on has no positions to put the rows back to.
            playback.next()
            removal.copy(totalAfter = null)
        }
        offers.show(
            message = deletedMessage(ids.size),
            onUndo = { undo(pending) },
            onExpire = { commit(pending) },
        )
    }

    /**
     * Takes the queue row at [position] out through [remove] and offers to put
     * it back. The offer is only made when the queue shrank by exactly the row:
     * an undo of a removal that did not happen would add a second copy.
     * [refresh] tells the queue page to read again once the row is back.
     */
    fun removeFromQueueWithUndo(
        position: Int,
        trackId: Long,
        playback: PlaybackControls,
        remove: () -> Unit,
        refresh: () -> Unit,
    ) {
        controls = playback
        scope.launch {
            val before = playback.upcomingTotal().getOrNull()
            remove()
            val after = playback.upcomingTotal().getOrNull()
            if (before == null || after != before - 1) return@launch
            val entry = QueueEntry(position, trackId)
            offers.show(
                message = QUEUE_REMOVED_MESSAGE,
                onUndo = { restoreQueueRow(entry, after, refresh) },
            )
        }
    }

    /** A library re-read up to [ticket] landed: what it confirmed gone may show no more. */
    fun libraryRefreshed(ticket: Int) {
        val landed = confirmedGone.filterValues { it <= ticket }.keys
        if (landed.isEmpty()) return
        confirmedGone.keys.removeAll(landed)
        hidden = hidden - landed
    }

    /** The screen is gone. Whatever is pending is dropped, never committed. */
    fun close() {
        offers.discard()
        queueEdits.clear()
        waitingForControls.clear()
        controls = null
        scope.cancel()
    }

    private fun undo(pending: PendingDeletion) {
        hidden = hidden - pending.ids.toSet()
        val edit = queueEdits.remove(pending.id) ?: return
        scope.launch {
            val removal = edit.await()
            val playback = controls ?: return@launch
            playback.restoreQueued(removal.entries, removal.totalAfter)
                .onFailure { error -> messages.say("Could not restore the queue: ${error.reason()}") }
        }
    }

    private fun restoreQueueRow(entry: QueueEntry, totalAfter: Long, refresh: () -> Unit) {
        val playback = controls ?: return
        scope.launch {
            playback.restoreQueued(listOf(entry), totalAfter)
                .onFailure { error -> messages.say("Could not restore the queue: ${error.reason()}") }
            refresh()
        }
    }

    private fun commit(pending: PendingDeletion) {
        val playback = controls
        if (playback == null) {
            // No activity to ask right now (a rotation is in progress): the
            // delete waits for the next one rather than being dropped.
            waitingForControls += pending
            return
        }
        scope.launch {
            queueEdits.remove(pending.id)?.await()
            delete(pending, playback)
        }
    }

    private fun delete(pending: PendingDeletion, playback: PlaybackControls) {
        val run = pending.sink.begin(deletingMessage(pending.ids.size))
        playback.deleteTracks(pending.ids) { outcome ->
            outcome.fold(
                onSuccess = { report ->
                    logTrashFailures(report)
                    settle(pending, report.removedIds.toSet())
                    run.finish(trashOutcomeMessage(report, pending.ids.size))
                },
                onFailure = { error ->
                    Log.w(TAG, "Could not delete tracks", error)
                    hidden = hidden - pending.ids.toSet()
                    run.finish("Could not delete tracks: ${error.reason()}")
                },
            )
        }
    }

    /** What failed to delete comes back at once; what did stays hidden until the re-read. */
    private fun settle(pending: PendingDeletion, gone: Set<Long>) {
        val ticket = latestRefreshTicket()
        gone.forEach { id -> confirmedGone[id] = ticket }
        hidden = hidden - (pending.ids.toSet() - gone)
    }

    private fun Throwable.reason() = message ?: "unknown error"
}
