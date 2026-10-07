package io.github.marvinbaudach.reprise.widget

import io.github.marvinbaudach.reprise.library.PlaybackKey
import io.github.marvinbaudach.reprise.library.TrackMetadata
import uniffi.reprise_android_ffi.AndroidPlaybackSnapshot
import uniffi.reprise_android_ffi.AndroidPlaybackState

/**
 * Everything the home-screen widget draws, and nothing it has to compute.
 *
 * The widget is rendered by the launcher's process from a snapshot, so it holds
 * plain values: the cover is a cache path, decoded only when the widget draws.
 */
internal data class WidgetNowPlaying(
    val trackId: Long?,
    val title: String,
    val artist: String,
    val isPlaying: Boolean,
    val artworkPath: String?,
    /**
     * Whether the playback service has a queue to carry on with. Playback that
     * ran out leaves none, and a transport button then has nothing to act on:
     * the widget sends the tap to the app instead of to a silent no-op.
     */
    val canResume: Boolean = true,
) {
    /** Nothing has been played yet: the widget shows the app icon and its name. */
    val isEmpty: Boolean get() = trackId == null

    companion object {
        val Empty = WidgetNowPlaying(
            trackId = null,
            title = "",
            artist = "",
            isPlaying = false,
            artworkPath = null,
            canResume = false,
        )
    }
}

/** What decides whether the widget has to be drawn again. */
internal data class WidgetStateKey(val trackId: Long?, val isPlaying: Boolean)

internal fun AndroidPlaybackSnapshot?.widgetKey(): WidgetStateKey =
    WidgetStateKey(this?.currentTrackId, this?.state == AndroidPlaybackState.PLAYING)

/**
 * The widget's state after [snapshot].
 *
 * A snapshot without a current track means playback ran out, not that the
 * widget should forget what it last showed: the last track stays, paused, with
 * nothing to resume. Only a widget that has never seen a track stays empty.
 */
internal fun widgetNowPlaying(
    snapshot: AndroidPlaybackSnapshot?,
    previous: WidgetNowPlaying,
    metadata: (PlaybackKey) -> TrackMetadata?,
    artworkPath: (trackUri: String) -> String?,
): WidgetNowPlaying {
    val trackId = snapshot?.currentTrackId
    val trackUri = snapshot?.currentTrackUri
    val playing = snapshot?.state == AndroidPlaybackState.PLAYING
    if (trackId == null || trackUri == null) {
        return previous.copy(isPlaying = false, canResume = false)
    }
    if (previous.trackId == trackId) {
        // A cover that was not there yet is asked for again: it lands a moment
        // after the track starts, and a miss must not stick for the whole track.
        val cover = previous.artworkPath ?: artworkPath(trackUri)
        return previous.copy(isPlaying = playing, artworkPath = cover, canResume = true)
    }
    val track = metadata(PlaybackKey(trackId, trackUri))
    return WidgetNowPlaying(
        trackId = trackId,
        title = track?.title.orEmpty(),
        artist = track?.artist.orEmpty(),
        isPlaying = playing,
        artworkPath = artworkPath(trackUri),
    )
}
