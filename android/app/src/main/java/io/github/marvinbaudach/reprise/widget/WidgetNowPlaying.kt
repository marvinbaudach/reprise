package io.github.marvinbaudach.reprise.widget

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
 * widget should forget what it last showed: the last track stays, paused.
 * Only a widget that has never seen a track stays empty.
 */
internal fun widgetNowPlaying(
    snapshot: AndroidPlaybackSnapshot?,
    previous: WidgetNowPlaying,
    metadata: (trackUri: String) -> TrackMetadata?,
    artworkPath: (trackUri: String) -> String?,
): WidgetNowPlaying {
    val trackId = snapshot?.currentTrackId
    val trackUri = snapshot?.currentTrackUri
    val playing = snapshot?.state == AndroidPlaybackState.PLAYING
    if (trackId == null || trackUri == null) {
        return previous.copy(isPlaying = false)
    }
    if (previous.trackId == trackId) {
        return previous.copy(isPlaying = playing)
    }
    val track = metadata(trackUri)
    return WidgetNowPlaying(
        trackId = trackId,
        title = track?.title.orEmpty(),
        artist = track?.artist.orEmpty(),
        isPlaying = playing,
        artworkPath = artworkPath(trackUri),
    )
}
