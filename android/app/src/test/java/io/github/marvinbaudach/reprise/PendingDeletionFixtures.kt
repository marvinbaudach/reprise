package io.github.marvinbaudach.reprise

import uniffi.reprise_android_ffi.AndroidRepeatMode
import uniffi.reprise_android_ffi.AndroidTrashReport

/** A scheduler the test fires by hand: nothing runs until [fire]. */
internal class ManualTimers {
    private val work = mutableListOf<() -> Unit>()
    val delays = mutableListOf<Long>()

    fun schedule(delayMs: Long, block: () -> Unit) {
        delays += delayMs
        work += block
    }

    fun fire(index: Int) = work[index]()

    val count get() = work.size

    fun fireAll() = work.toList().forEach { it() }
}

internal class RecordingDeletionMessages : DeletionMessages {
    val lines = mutableListOf<String>()

    override fun say(text: String) {
        lines += text
    }

    override fun begin(text: String): DeletionRun {
        lines += text
        return DeletionRun { outcome -> lines += outcome }
    }
}

/**
 * A playback transport whose upcoming queue is a plain list, answering every
 * call at once on the calling thread.
 */
internal class FakeQueueControls(
    initial: List<Long>,
    private val outcome: Result<AndroidTrashReport>? = null,
    /** How many deletes answer "still connecting" before one is carried out. */
    var connectFailures: Int = 0,
    /** Queue removals wait in [heldRemovals] until the test releases them. */
    var holdRemovals: Boolean = false,
) : PlaybackControls {
    val upcoming = initial.toMutableList()
    val deleted = mutableListOf<List<Long>>()
    val heldRemovals = mutableListOf<() -> Unit>()
    var skips = 0
    var nexts = 0

    override fun togglePause() = Unit
    override fun next() {
        nexts += 1
    }

    override fun skipCurrentOrStop() {
        skips += 1
    }

    override fun previous() = Unit
    override fun seekTo(positionMs: Long) = Unit
    override fun setShuffle(enabled: Boolean) = Unit
    override fun setRepeat(mode: AndroidRepeatMode) = Unit
    override fun setFavourite(trackId: Long, favourite: Boolean, report: (String?) -> Unit) =
        report(null)

    override fun loadUpcomingTracks(
        window: LibraryWindowRange,
        report: (Result<LibraryWindow<LibraryTrack>>) -> Unit,
    ) {
        val rows = upcoming
            .drop(window.offset.toInt().coerceAtLeast(0))
            .take(window.limit.toInt())
            .map { id -> configurationTestTrack(id, "Track $id") }
        report(
            Result.success(
                LibraryWindow(
                    total = upcoming.size.toLong(),
                    rows = rows,
                    hasMore = window.offset + rows.size < upcoming.size,
                ),
            ),
        )
    }

    override fun removeUpcomingTrack(
        position: Int,
        expectedTrackId: Long,
        report: (Result<Boolean>) -> Unit,
    ) {
        val remove = {
            val matches = upcoming.getOrNull(position) == expectedTrackId
            if (matches) upcoming.removeAt(position)
            report(Result.success(matches))
        }
        if (holdRemovals) heldRemovals += remove else remove()
    }

    fun releaseRemovals() {
        val held = heldRemovals.toList()
        heldRemovals.clear()
        held.forEach { it() }
    }

    override fun moveUpcomingTrack(
        fromPosition: Int,
        expectedTrackId: Long,
        toPosition: Int,
        report: (Result<Boolean>) -> Unit,
    ) {
        val matches = upcoming.getOrNull(fromPosition) == expectedTrackId &&
            toPosition in upcoming.indices
        if (matches && fromPosition != toPosition) {
            upcoming.add(toPosition, upcoming.removeAt(fromPosition))
        }
        report(Result.success(matches && fromPosition != toPosition))
    }

    override fun queueTracksNext(trackIds: List<Long>, report: (Result<UInt>) -> Unit) {
        upcoming.addAll(0, trackIds)
        report(Result.success(trackIds.size.toUInt()))
    }

    override fun queueTracksLast(trackIds: List<Long>, report: (Result<UInt>) -> Unit) {
        upcoming.addAll(trackIds)
        report(Result.success(trackIds.size.toUInt()))
    }

    override fun deleteTracks(
        trackIds: List<Long>,
        report: (Result<AndroidTrashReport>) -> Unit,
    ) {
        if (connectFailures > 0) {
            connectFailures -= 1
            report(Result.failure(IllegalStateException(PLAYBACK_STILL_CONNECTING)))
            return
        }
        deleted.add(trackIds)
        upcoming.removeAll(trackIds.toSet())
        report(outcome ?: Result.success(AndroidTrashReport(trackIds, emptyList())))
    }
}

/**
 * The library surface as a deferred delete sees it: a view model whose undo
 * window is passed by hand. Provide [surface] as `LocalDeletionMessages`, then
 * call [passTheWindow] where the six seconds would have run out.
 */
internal class DeletionHarness {
    val timers = ManualTimers()
    val surface = MobileSurfaceViewModel(scheduleAfter = timers::schedule)

    fun passTheWindow() = timers.fireAll()
}

/** A dispatcher the test runs by hand, so a coroutine can be caught in the middle. */
internal class ManualDispatcher : kotlinx.coroutines.CoroutineDispatcher() {
    private val queue = ArrayDeque<Runnable>()

    override fun dispatch(context: kotlin.coroutines.CoroutineContext, block: Runnable) {
        queue.addLast(block)
    }

    fun runAll() {
        while (queue.isNotEmpty()) queue.removeFirst().run()
    }
}
