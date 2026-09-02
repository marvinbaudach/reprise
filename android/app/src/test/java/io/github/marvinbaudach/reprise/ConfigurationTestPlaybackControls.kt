package io.github.marvinbaudach.reprise

import uniffi.reprise_android_ffi.AndroidRepeatMode

internal class ConfigurationTestPlaybackControls(
    private val store: (Long, Int) -> Unit = { _, _ -> },
    private val loadUpcoming: (LibraryWindowRange) -> LibraryWindow<LibraryTrack> = {
        LibraryWindow.empty()
    },
    private val playUpcoming: (Int, Long) -> Boolean = { _, _ -> false },
    private val moveUpcoming: (Int, Long, Int) -> Boolean = { _, _, _ -> false },
    private val removeUpcoming: (Int, Long) -> Boolean = { _, _ -> false },
    private val startSleepTimer: (SleepTimerSelection) -> Unit = {},
    private val cancelSleepTimer: () -> Unit = {},
) : PlaybackControls {
    private var deferredUpcomingOffset: Long? = null
    private val deferredUpcomingLoads = mutableListOf<() -> Unit>()
    val seekPositions = mutableListOf<Long>()
    val ratingRequests = mutableListOf<Pair<Long, Int>>()
    val playUpcomingRequests = mutableListOf<Pair<Int, Long>>()
    val moveUpcomingRequests = mutableListOf<Triple<Int, Long, Int>>()
    val removeUpcomingRequests = mutableListOf<Pair<Int, Long>>()
    val loadUpcomingRequests = mutableListOf<LibraryWindowRange>()
    var queuePreviousCalls = 0

    /** What the write answers with; null is the database agreeing. */
    var ratingFailure: String? = null

    fun deferUpcomingLoad(offset: Long) {
        deferredUpcomingOffset = offset
    }

    fun completeDeferredUpcomingLoads() {
        deferredUpcomingOffset = null
        deferredUpcomingLoads.toList().also { deferredUpcomingLoads.clear() }.forEach { it() }
    }

    override fun togglePause() = Unit
    override fun next() = Unit
    override fun previous() = Unit
    override fun previousInQueueOrder() {
        queuePreviousCalls += 1
    }
    override fun seekTo(positionMs: Long) {
        seekPositions += positionMs
    }
    override fun setShuffle(enabled: Boolean) = Unit
    override fun setRepeat(mode: AndroidRepeatMode) = Unit
    override fun setFavourite(trackId: Long, favourite: Boolean, report: (String?) -> Unit) {
        val rating = if (favourite) 5 else 0
        ratingRequests += trackId to rating
        if (ratingFailure == null) {
            store(trackId, rating)
        }
        report(ratingFailure)
    }

    override fun loadUpcomingTracks(
        window: LibraryWindowRange,
        report: (Result<LibraryWindow<LibraryTrack>>) -> Unit,
    ) {
        loadUpcomingRequests += window
        val answer = Result.success(loadUpcoming(window))
        if (window.offset == deferredUpcomingOffset) {
            deferredUpcomingOffset = null
            deferredUpcomingLoads += { report(answer) }
        } else {
            report(answer)
        }
    }

    override fun playUpcomingTrackNow(
        position: Int,
        expectedTrackId: Long,
        report: (Result<Boolean>) -> Unit,
    ) {
        playUpcomingRequests += position to expectedTrackId
        report(Result.success(playUpcoming(position, expectedTrackId)))
    }

    override fun moveUpcomingTrack(
        fromPosition: Int,
        expectedTrackId: Long,
        toPosition: Int,
        report: (Result<Boolean>) -> Unit,
    ) {
        moveUpcomingRequests += Triple(fromPosition, expectedTrackId, toPosition)
        report(Result.success(moveUpcoming(fromPosition, expectedTrackId, toPosition)))
    }

    override fun removeUpcomingTrack(
        position: Int,
        expectedTrackId: Long,
        report: (Result<Boolean>) -> Unit,
    ) {
        removeUpcomingRequests += position to expectedTrackId
        report(Result.success(removeUpcoming(position, expectedTrackId)))
    }

    override fun startSleepTimer(selection: SleepTimerSelection) = startSleepTimer.invoke(selection)

    override fun cancelSleepTimer() = cancelSleepTimer.invoke()
}
