package de.reprise.spike

import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors
import java.util.concurrent.RejectedExecutionException
import uniffi.reprise_android_ffi.AndroidRepeatMode
import uniffi.reprise_android_ffi.AndroidTrashReport
import uniffi.reprise_android_ffi.TrashAction

internal const val PLAYBACK_QUERIES_STOPPED = "playback controls are closing"

/**
 * One ordered lane for queue reads and edits.
 *
 * A single worker is load-bearing: position-based edits are separate calls at
 * the FFI boundary, so submitting them to a pool could apply a later gesture
 * before the earlier one. Reads share the lane so a refresh observes every
 * edit submitted before it.
 */
internal class PlaybackQueryRunner(
    private val worker: ExecutorService = Executors.newSingleThreadExecutor { runnable ->
        Thread(runnable, "reprise-playback-queries")
    },
) {
    fun <T> query(operation: () -> T, report: (Result<T>) -> Unit) {
        try {
            worker.execute { report(runCatching(operation)) }
        } catch (rejected: RejectedExecutionException) {
            report(Result.failure(IllegalStateException(PLAYBACK_QUERIES_STOPPED, rejected)))
        }
    }

    fun shutdown() = worker.shutdown()
}

/** MainActivity's service-backed implementation, kept out of its composition root. */
internal class ActivityPlaybackControls(
    private val command: (String, ReprisePlaybackService.() -> Unit) -> Unit,
    private val connectedService: () -> ReprisePlaybackService?,
    private val postToMain: (() -> Unit) -> Unit,
    private val setFavouriteAction: (Long, Boolean, (String?) -> Unit) -> Unit,
    private val trashAction: TrashAction,
    private val queries: PlaybackQueryRunner = PlaybackQueryRunner(),
) : PlaybackControls {
    override fun togglePause() = command("change playback state") { togglePause() }

    override fun next() = command("skip to the next track") { next() }

    override fun previous() = command("return to the previous track") { previous() }

    override fun seekTo(positionMs: Long) = command("seek") { seekTo(positionMs) }

    override fun setShuffle(enabled: Boolean) = command("change shuffle") {
        setShuffle(enabled)
    }

    override fun setRepeat(mode: AndroidRepeatMode) = command("change repeat") {
        setRepeat(mode)
    }

    override fun setFavourite(
        trackId: Long,
        favourite: Boolean,
        report: (String?) -> Unit,
    ) = setFavouriteAction(trackId, favourite, report)

    override fun loadUpcomingTracks(
        window: LibraryWindowRange,
        report: (Result<LibraryWindow<LibraryTrack>>) -> Unit,
    ) = query(report) { upcomingTracks(window) }

    override fun playUpcomingTrackNow(
        position: Int,
        expectedTrackId: Long,
        report: (Result<Boolean>) -> Unit,
    ) = query(report) { playUpcomingTrackNow(position, expectedTrackId) }

    override fun moveUpcomingTrack(
        fromPosition: Int,
        expectedTrackId: Long,
        toPosition: Int,
        report: (Result<Boolean>) -> Unit,
    ) = query(report) { moveUpcomingTrack(fromPosition, expectedTrackId, toPosition) }

    override fun removeUpcomingTrack(
        position: Int,
        expectedTrackId: Long,
        report: (Result<Boolean>) -> Unit,
    ) = query(report) { removeUpcomingTrack(position, expectedTrackId) }

    override fun queueTracksNext(trackIds: List<Long>, report: (Result<UInt>) -> Unit) =
        query(report) { queueTracksNext(trackIds) }

    override fun queueTracksLast(trackIds: List<Long>, report: (Result<UInt>) -> Unit) =
        query(report) { queueTracksLast(trackIds) }

    override fun deleteTracks(
        trackIds: List<Long>,
        report: (Result<AndroidTrashReport>) -> Unit,
    ) = query(report) { trashTracks(trackIds, trashAction) }

    override fun startSleepTimer(selection: SleepTimerSelection) = command("start sleep timer") {
        startSleepTimer(selection)
    }

    override fun cancelSleepTimer() = command("cancel sleep timer") { cancelSleepTimer() }

    fun shutdown() = queries.shutdown()

    private fun <T> query(
        report: (Result<T>) -> Unit,
        operation: ReprisePlaybackService.() -> T,
    ) {
        val service = connectedService()
        if (service == null) {
            report(Result.failure(IllegalStateException("playback is still connecting")))
            return
        }
        queries.query(
            operation = { service.operation() },
        ) { outcome ->
            postToMain { report(outcome) }
        }
    }
}
