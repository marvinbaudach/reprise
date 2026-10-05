package io.github.marvinbaudach.reprise

import kotlin.coroutines.resume
import kotlinx.coroutines.suspendCancellableCoroutine

/** One upcoming-queue row a delete or a removal took out, and where it sat. */
internal data class QueueEntry(val position: Int, val trackId: Long)

/**
 * What [removeQueued] took out of the queue.
 *
 * [totalAfter] is how many upcoming rows were left, and is what a later undo
 * compares the queue against: only a queue of that same size still has the
 * positions [entries] were read from. Null means "do not trust positions".
 */
internal data class QueueRemoval(val entries: List<QueueEntry>, val totalAfter: Long?) {
    companion object {
        val NOTHING = QueueRemoval(emptyList(), totalAfter = null)
    }
}

private suspend fun <T> awaitReport(start: ((Result<T>) -> Unit) -> Unit): Result<T> =
    suspendCancellableCoroutine { continuation ->
        start { outcome -> if (continuation.isActive) continuation.resume(outcome) }
    }

/** Every upcoming row, read in the windows the core answers. */
internal suspend fun PlaybackControls.readUpcoming(): Result<List<LibraryTrack>> {
    val rows = mutableListOf<LibraryTrack>()
    while (true) {
        val window = awaitReport<LibraryWindow<LibraryTrack>> { report ->
            loadUpcomingTracks(LibraryWindowRange(rows.size.toLong(), RELOAD_CHUNK_LIMIT), report)
        }.getOrElse { return Result.failure(it) }
        rows += window.rows
        if (!window.hasMore || window.rows.isEmpty()) return Result.success(rows)
    }
}

/** How many rows the upcoming queue holds. */
internal suspend fun PlaybackControls.upcomingTotal(): Result<Long> =
    awaitReport<LibraryWindow<LibraryTrack>> { report ->
        loadUpcomingTracks(LibraryWindowRange(0, 1), report)
    }.map(LibraryWindow<LibraryTrack>::total)

/**
 * Takes every upcoming row whose track is in [ids] out of the queue.
 *
 * Rows go back to front, so the positions still to be used stay valid as the
 * queue shrinks; each removal carries the id it expects, so a queue that moved
 * on meanwhile refuses rather than removes the wrong row. A queue that cannot
 * be read is left alone: the delete itself takes the ids out of it when it runs.
 */
internal suspend fun PlaybackControls.removeQueued(ids: Set<Long>): QueueRemoval {
    val upcoming = readUpcoming().getOrElse { return QueueRemoval.NOTHING }
    val wanted = upcoming.withIndex()
        .filter { (_, track) -> track.id in ids }
        .map { (position, track) -> QueueEntry(position, track.id) }
    val removed = mutableListOf<QueueEntry>()
    for (entry in wanted.asReversed()) {
        val done = awaitReport<Boolean> { report ->
            removeUpcomingTrack(entry.position, entry.trackId, report)
        }.getOrDefault(false)
        if (done) removed += entry
    }
    removed.reverse()
    return QueueRemoval(removed, totalAfter = (upcoming.size - removed.size).toLong())
}

/**
 * Puts [entries] back into the queue.
 *
 * When the queue still has the [expectedTotal] rows it had right after the
 * removal, nothing was added or played since, and every row returns to its old
 * position: appended, then moved front to back, which keeps each later row's
 * index valid. Otherwise the positions mean nothing and the rows come back as
 * the next ones to play.
 */
internal suspend fun PlaybackControls.restoreQueued(
    entries: List<QueueEntry>,
    expectedTotal: Long?,
): Result<Unit> {
    if (entries.isEmpty()) return Result.success(Unit)
    val total = upcomingTotal().getOrElse { return Result.failure(it) }
    val ids = entries.map(QueueEntry::trackId)
    if (expectedTotal == null || total != expectedTotal) {
        return awaitReport<UInt> { report -> queueTracksNext(ids, report) }.map { }
    }
    awaitReport<UInt> { report -> queueTracksLast(ids, report) }
        .onFailure { return Result.failure(it) }
    entries.forEachIndexed { offset, entry ->
        val from = total.toInt() + offset
        if (entry.position < from) {
            awaitReport<Boolean> { report ->
                moveUpcomingTrack(from, entry.trackId, entry.position, report)
            }.onFailure { return Result.failure(it) }
        }
    }
    return Result.success(Unit)
}
