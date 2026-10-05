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
) : PlaybackControls {
    val upcoming = initial.toMutableList()
    val deleted = mutableListOf<List<Long>>()
    var skips = 0

    override fun togglePause() = Unit
    override fun next() {
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
        val matches = upcoming.getOrNull(position) == expectedTrackId
        if (matches) upcoming.removeAt(position)
        report(Result.success(matches))
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
