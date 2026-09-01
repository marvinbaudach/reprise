package de.reprise.spike

import androidx.compose.runtime.Immutable
import androidx.compose.runtime.staticCompositionLocalOf
import uniffi.reprise_android_ffi.AndroidRepeatMode
import uniffi.reprise_android_ffi.AndroidTrashReport

/**
 * Every transport command the surface can issue, in one place.
 *
 * These are used by exactly two leaves — [NowPlayingSheet] and the mini player
 * inside [LibraryBottomFrame] — which sit on opposite sides of the screen's
 * composition. Passing them as parameters meant seven arguments threaded
 * through `LibraryScreen` and `BrowseScreen`, neither of which calls a single
 * one of them. This codebase already answered that question once for artwork
 * ([LocalTrackArtwork]); this is the same answer for the transport.
 */
@Immutable
internal interface PlaybackControls {
    fun togglePause()

    fun next()

    fun previous()

    /** Moves to the preceding item in queue order, independent of history. */
    fun previousInQueueOrder() = previous()

    fun seekTo(positionMs: Long)

    fun setShuffle(enabled: Boolean)

    fun setRepeat(mode: AndroidRepeatMode)

    /**
     * Saves one favourite state and answers through [report]: the failure to
     * show, or null when the database accepted it.
     *
     * A callback rather than a return value because the write does not belong
     * on the thread that raises the tap — it is a SQLite transaction that can
     * queue behind a whole folder scan (see [RatingWriter]). What that must not
     * cost is *when* the heart moves: [report] is what the heart waits for, so it
     * still moves only after the database agreed, never before and never in
     * hope. [report] is called on the main thread, exactly once per tap.
     */
    fun setFavourite(trackId: Long, favourite: Boolean, report: (String?) -> Unit)

    fun loadUpcomingTracks(
        window: LibraryWindowRange,
        report: (Result<LibraryWindow<LibraryTrack>>) -> Unit,
    ) = report(Result.failure(IllegalStateException("playback is not connected")))

    fun playUpcomingTrackNow(
        position: Int,
        expectedTrackId: Long,
        report: (Result<Boolean>) -> Unit,
    ) = report(Result.failure(IllegalStateException("playback is not connected")))

    fun moveUpcomingTrack(
        fromPosition: Int,
        expectedTrackId: Long,
        toPosition: Int,
        report: (Result<Boolean>) -> Unit,
    ) = report(Result.failure(IllegalStateException("playback is not connected")))

    fun removeUpcomingTrack(
        position: Int,
        expectedTrackId: Long,
        report: (Result<Boolean>) -> Unit,
    ) = report(Result.failure(IllegalStateException("playback is not connected")))

    fun queueTracksNext(trackIds: List<Long>, report: (Result<UInt>) -> Unit) =
        report(Result.failure(IllegalStateException("playback is not connected")))

    fun queueTracksLast(trackIds: List<Long>, report: (Result<UInt>) -> Unit) =
        report(Result.failure(IllegalStateException("playback is not connected")))

    fun deleteTracks(
        trackIds: List<Long>,
        report: (Result<AndroidTrashReport>) -> Unit,
    ) = report(Result.failure(IllegalStateException("playback is not connected")))

    fun playTrackIds(trackIds: List<Long>, startIndex: Int) = Unit

    fun startSleepTimer(selection: SleepTimerSelection) = Unit

    fun cancelSleepTimer() = Unit
}

/**
 * What the controls do when nobody has provided any: nothing.
 *
 * [setFavourite] is the one command that has to answer, and it answers with a
 * failure rather than the `null` that means "saved". A preview or a test that
 * rates a track must not come away believing a write happened — a default that
 * pretends is worse than no default at all.
 */
internal object DisconnectedPlaybackControls : PlaybackControls {
    override fun togglePause() = Unit

    override fun next() = Unit

    override fun previous() = Unit

    override fun previousInQueueOrder() = Unit

    override fun seekTo(positionMs: Long) = Unit

    override fun setShuffle(enabled: Boolean) = Unit

    override fun setRepeat(mode: AndroidRepeatMode) = Unit

    override fun setFavourite(trackId: Long, favourite: Boolean, report: (String?) -> Unit) =
        report("Could not save rating: playback is not connected.")
}

/** No transport unless an activity provides one — previews stay honest. */
internal val LocalPlaybackControls =
    staticCompositionLocalOf<PlaybackControls> { DisconnectedPlaybackControls }
