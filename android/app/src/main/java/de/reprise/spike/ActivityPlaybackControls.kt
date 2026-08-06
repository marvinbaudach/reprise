package de.reprise.spike

import uniffi.reprise_android_ffi.AndroidRepeatMode

/** MainActivity's service-backed implementation, kept out of its composition root. */
internal class ActivityPlaybackControls(
    private val command: (String, ReprisePlaybackService.() -> Unit) -> Unit,
    private val connectedService: () -> ReprisePlaybackService?,
    private val postToMain: (() -> Unit) -> Unit,
    private val setFavouriteAction: (Long, Boolean, (String?) -> Unit) -> Unit,
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

    private fun <T> query(
        report: (Result<T>) -> Unit,
        operation: ReprisePlaybackService.() -> T,
    ) {
        val service = connectedService()
        if (service == null) {
            report(Result.failure(IllegalStateException("playback is still connecting")))
            return
        }
        Thread {
            val outcome = runCatching { service.operation() }
            postToMain { report(outcome) }
        }.start()
    }
}
