package io.github.marvinbaudach.reprise

import android.util.Log
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import uniffi.reprise_android_ffi.AndroidTrashReport
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

/**
 * What the offer says while the delete is still undoable: said in the future
 * tense, because nothing has been deleted yet.
 */
internal fun willBeDeletedMessage(count: Int): String =
    "$count ${if (count == 1) "track" else "tracks"} will be deleted"

internal const val QUEUE_REMOVED_MESSAGE = "Removed from queue"

internal const val DELETION_UNAVAILABLE = "Deleting is not available here."

internal const val EVERYTHING_TAPPED_IS_BEING_DELETED = "Those tracks are about to be deleted."

/** What `ActivityPlaybackControls` answers while its service is not bound. */
internal const val PLAYBACK_STILL_CONNECTING = "playback is still connecting"

/** How soon, and how often, a parked action looks again for a bound service. */
internal const val RECONNECT_RETRY_MS = 500L
internal const val RECONNECT_ATTEMPTS = 20

private fun Throwable.isStillConnecting() = message == PLAYBACK_STILL_CONNECTING

/**
 * Deletes that are said, shown and undoable first, and carried out later.
 *
 * Choosing "Delete from device…" hides the tracks and takes them out of the
 * upcoming queue at once, and offers Undo for the window [undoWindowMs] names.
 * Only when the window passes — or a newer offer takes the slot — does the file
 * deletion run, through the same `deleteTracks` path as before. A process that
 * dies in the window deletes nothing: the pending state lives in memory only,
 * and [close] discards it without committing.
 *
 * Whatever needs the transport — a commit, an undo's queue restore — runs only
 * while the window is in the foreground and an activity's transport is bound.
 * Otherwise it is parked and runs when both hold again, and a transport that
 * answers "still connecting" is asked again shortly, since a service binds a
 * moment after its activity starts.
 *
 * The same slot carries the queue's "Removed from queue · Undo"
 * ([removeFromQueueWithUndo]), because the host shows one line at a time; it
 * waits for a delete's window rather than ending it.
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
    private val parked = mutableListOf<() -> Unit>()
    private var controls: PlaybackControls? = null
    private var foreground = true
    private var retrying = false
    private var closed = false
    private var lastId = 0L

    /** How long the next offer stays on: the host raises it for accessibility settings. */
    var undoWindowMs: () -> Long = { UNDO_WINDOW_MS }

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

    /** Tells the listener that what they asked for is, all of it, about to be deleted. */
    fun sayEverythingIsBeingDeleted() = messages.say(EVERYTHING_TAPPED_IS_BEING_DELETED)

    /** The transport a commit will use; the host keeps it current across rotation. */
    fun bind(playback: PlaybackControls) {
        controls = playback
        drain()
    }

    fun unbind(playback: PlaybackControls) {
        if (controls === playback) controls = null
    }

    /**
     * Whether the window is on screen. A backgrounded window parks what needs
     * the transport and runs it when this turns true again.
     */
    fun setForeground(visible: Boolean) {
        foreground = visible
        if (visible) drain()
    }

    /**
     * Hides [trackIds], takes them out of the upcoming queue, skips on when one
     * of them is playing, and offers the undo. A second call inside the window
     * commits the first delete: its undo is gone with the slot.
     *
     * [playback] edits the queue now. It is kept as the transport only when none
     * is bound: a wrapper handed down to a row must never replace the one the
     * host binds, or the host could not unbind it again.
     */
    fun begin(
        trackIds: List<Long>,
        playback: PlaybackControls,
        sink: DeletionMessages = messages,
    ) {
        val ids = withoutHidden(trackIds.distinct())
        if (ids.isEmpty()) return
        if (controls == null) controls = playback
        val pending = PendingDeletion(++lastId, ids, sink)
        hidden = hidden + ids
        val skipsCurrent = currentTrackId()?.let { it in ids } == true
        queueEdits[pending.id] = scope.async {
            val removal = playback.removeQueued(ids.toSet())
            if (!skipsCurrent) return@async removal
            // The queue's positions are relative to the playing track, so
            // skipping on has to wait until they were used; and a queue that
            // has moved on has no positions to put the rows back to. The Next
            // gesture would do nothing on the last track (PLAY-8b), yet a
            // track that is going away has to be left.
            playback.skipCurrentOrStop()
            removal.copy(totalAfter = null)
        }
        offers.show(
            message = willBeDeletedMessage(ids.size),
            windowMs = undoWindowMs(),
            holdsSlot = true,
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
        if (controls == null) controls = playback
        scope.launch {
            val before = playback.upcomingTotal().getOrNull()
            remove()
            val after = playback.upcomingTotal().getOrNull()
            if (before == null || after != before - 1) return@launch
            val removal = QueueRemoval(listOf(QueueEntry(position, trackId)), after)
            offers.show(
                message = QUEUE_REMOVED_MESSAGE,
                windowMs = undoWindowMs(),
                onUndo = { restoreWhenReady(removal, refresh) },
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

    /**
     * The screen is gone. Nothing pending is committed, and what was parked is
     * dropped.
     *
     * The queue rows a pending delete took out are put back when a transport is
     * still bound. Normally none is: an activity unbinds on stop, before its
     * view model is cleared, and the queue then belongs to the service alone,
     * which is asked nothing — those rows stay out, and the files stay too.
     */
    fun close() {
        closed = true
        offers.discard()
        parked.clear()
        val playback = controls
        controls = null
        val edits = queueEdits.values.toList()
        queueEdits.clear()
        if (playback == null || edits.isEmpty()) {
            scope.cancel()
            return
        }
        scope.launch {
            try {
                edits.forEach { edit ->
                    val removal = edit.await()
                    playback.restoreQueued(removal.entries, removal.totalAfter)
                }
            } finally {
                scope.cancel()
            }
        }
    }

    private fun undo(pending: PendingDeletion) {
        hidden = hidden - pending.ids.toSet()
        val edit = queueEdits.remove(pending.id) ?: return
        scope.launch { restoreWhenReady(edit.await()) }
    }

    /** The transport, when this window may use it: on screen, and an activity's is bound. */
    private fun readyControls(): PlaybackControls? = controls.takeIf { foreground }

    private fun whenReady(action: (PlaybackControls) -> Unit) {
        val playback = readyControls()
        if (playback == null) {
            // A rotation is in progress, or the window is in the background:
            // the work waits for the next activity rather than being dropped.
            parked += { whenReady(action) }
            return
        }
        action(playback)
    }

    private fun drain() {
        if (readyControls() == null || parked.isEmpty()) return
        val due = parked.toList()
        parked.clear()
        due.forEach { it() }
    }

    /** Parks [again] and looks for a bound service every [RECONNECT_RETRY_MS], a while. */
    private fun parkUntilConnected(again: () -> Unit) {
        parked += again
        if (retrying || closed) return
        retrying = true
        retryAfterDelay(RECONNECT_ATTEMPTS)
    }

    private fun retryAfterDelay(attemptsLeft: Int) {
        offers.after(RECONNECT_RETRY_MS) {
            if (closed) return@after
            drain()
            if (parked.isNotEmpty() && attemptsLeft > 1) {
                retryAfterDelay(attemptsLeft - 1)
            } else {
                retrying = false
            }
        }
    }

    private fun restoreWhenReady(removal: QueueRemoval, refresh: () -> Unit = {}) {
        whenReady { playback ->
            scope.launch {
                val outcome = playback.restoreQueued(removal.entries, removal.totalAfter)
                val error = outcome.exceptionOrNull()
                if (error?.isStillConnecting() == true) {
                    parkUntilConnected { restoreWhenReady(removal, refresh) }
                    return@launch
                }
                error?.let { messages.say("Could not restore the queue: ${it.reason()}") }
                refresh()
            }
        }
    }

    private fun commit(pending: PendingDeletion) {
        whenReady {
            scope.launch {
                queueEdits[pending.id]?.await()
                // The activity may have gone while the queue edit was finishing.
                whenReady { playback -> delete(pending, playback) }
            }
        }
    }

    /**
     * Asks the transport to delete. A transport that has no service yet answers
     * at once, before anything started; that is not a failure to show, and no
     * "Deleting…" line is raised for it — the delete is parked and asked again.
     */
    private fun delete(pending: PendingDeletion, playback: PlaybackControls) {
        var run: DeletionRun? = null
        var early: Result<AndroidTrashReport>? = null
        playback.deleteTracks(pending.ids) { outcome ->
            val started = run
            if (started == null) early = outcome else answer(pending, started, outcome)
        }
        val answeredAtOnce = early
        if (answeredAtOnce?.exceptionOrNull()?.isStillConnecting() == true) {
            parkUntilConnected { commit(pending) }
            return
        }
        // From here on the files are the transport's: the rows are not ours to restore.
        queueEdits.remove(pending.id)
        val started = pending.sink.begin(deletingMessage(pending.ids.size))
        run = started
        if (answeredAtOnce != null) answer(pending, started, answeredAtOnce)
    }

    private fun answer(
        pending: PendingDeletion,
        run: DeletionRun,
        outcome: Result<AndroidTrashReport>,
    ) {
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

    /** What failed to delete comes back at once; what did stays hidden until the re-read. */
    private fun settle(pending: PendingDeletion, gone: Set<Long>) {
        val ticket = latestRefreshTicket()
        gone.forEach { id -> confirmedGone[id] = ticket }
        hidden = hidden - (pending.ids.toSet() - gone)
    }

    private fun Throwable.reason() = message ?: "unknown error"
}
