package de.reprise.spike

import androidx.compose.runtime.Immutable
import androidx.compose.runtime.staticCompositionLocalOf
import uniffi.reprise_android_ffi.AndroidRepeatMode

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

    fun seekTo(positionMs: Long)

    fun setShuffle(enabled: Boolean)

    fun setRepeat(mode: AndroidRepeatMode)

    /** The failure to show, or null when the rating was saved. */
    fun setRating(trackId: Long, rating: Int): String?
}

/**
 * What the controls do when nobody has provided any: nothing.
 *
 * [setRating] is the one command that has to answer, and it answers with a
 * failure rather than the `null` that means "saved". A preview or a test that
 * rates a track must not come away believing a write happened — a default that
 * pretends is worse than no default at all.
 */
internal object DisconnectedPlaybackControls : PlaybackControls {
    override fun togglePause() = Unit

    override fun next() = Unit

    override fun previous() = Unit

    override fun seekTo(positionMs: Long) = Unit

    override fun setShuffle(enabled: Boolean) = Unit

    override fun setRepeat(mode: AndroidRepeatMode) = Unit

    override fun setRating(trackId: Long, rating: Int): String =
        "Could not save rating: playback is not connected."
}

/** No transport unless an activity provides one — previews stay honest. */
internal val LocalPlaybackControls =
    staticCompositionLocalOf<PlaybackControls> { DisconnectedPlaybackControls }
