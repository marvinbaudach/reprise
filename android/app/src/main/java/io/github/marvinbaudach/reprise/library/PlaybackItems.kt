package io.github.marvinbaudach.reprise.library

import android.net.Uri
import android.util.Log
import androidx.media3.common.C
import androidx.media3.common.MediaItem
import uniffi.reprise_android_ffi.AndroidPlaybackSegment

private const val TAG = "RepriseItems"

/** How many tracks' metadata and covers are remembered; a queue's worth of neighbours. */
private const val REMEMBERED_TRACKS = 64

/**
 * Who a queued item is: its library row, and the uri it plays from.
 *
 * The tracks a CUE sheet cuts from one file share their uri, so the uri alone
 * names the file, never the track. An item the Core started without a row (a
 * stream) has no [trackId] and is known by its uri.
 */
internal data class PlaybackKey(val trackId: Long?, val uri: String)

/**
 * Everything an item is built from apart from its gain: who it is and which
 * stretch of the file it plays. Carried as the item's tag, so the item can be
 * rebuilt from itself when more becomes known about it.
 */
internal data class PlaybackRequest(
    val key: PlaybackKey,
    val segment: AndroidPlaybackSegment? = null,
)

/**
 * Builds the [MediaItem] the player plays for a request, and remembers what it
 * learned about each track.
 *
 * What the library says about a track and where its cover lives cost a blocking
 * read each, but they do not change while a track is queued. Remembering them
 * means an item built a second time (a track tapped again, repeat-one, the
 * gapless next item, a notification skip) is complete at once and asks the
 * library nothing. Metadata is remembered per track; a cover per file, since
 * every track of a CUE file shows the file's cover.
 *
 * Safe to call from any thread: the player's own thread builds items while the
 * Core's and the cover loader's threads teach it.
 */
internal class PlaybackItems(
    private val metadata: TrackMetadataResolver,
    /** The id the browse tree lists a track under, so a browser can mark the playing row. */
    private val mediaIdOf: (trackId: Long) -> String? = { null },
) {
    private val tracks = LruMap<PlaybackKey, TrackMetadata>()
    private val covers = LruMap<String, Uri>()

    fun isKnown(key: PlaybackKey): Boolean = synchronized(this) { tracks.containsKey(key) }

    /** Asks the library about [key] and remembers the answer; blocks, so never on the player's thread. */
    fun resolve(key: PlaybackKey): Boolean {
        val track = try {
            metadata.resolve(key)
        } catch (error: Exception) {
            // Metadata is decoration: a library that cannot answer must not
            // stop the music, only leave the notification without a title.
            Log.w(TAG, "Could not read the metadata for a playing track", error)
            null
        } ?: return false
        synchronized(this) { tracks[key] = track }
        return true
    }

    /** Remembers [cover] for the file at [uri]; a cover that is already known is kept. */
    fun rememberCover(uri: String, cover: Uri) {
        synchronized(this) { if (!covers.containsKey(uri)) covers[uri] = cover }
    }

    /** The item for [request] from what is remembered; a track nothing is known about still plays, bare. */
    fun build(request: PlaybackRequest): MediaItem {
        val uri = request.key.uri
        val (track, cover) = synchronized(this) { tracks[request.key] to covers[uri] }
        return playbackMediaItem(uri, track?.copy(artworkUri = cover), track?.let { mediaIdOf(it.trackId) })
            .buildUpon()
            .setTag(request)
            .setClippingConfiguration(request.segment.clipping())
            .build()
    }
}

/**
 * The clip of its file a CUE track plays. Media3 then counts the item's
 * position and duration from the clip's start. A segment without an end is its
 * file's last track and plays to the end of the source, whatever duration the
 * file's metadata claims; no segment plays the whole file. Every segment claims
 * a key-frame start because Media3 otherwise reports an initial discontinuity
 * for formats it does not classify as all-sync, including Opus, which restarts
 * the renderer and leaves an audible gap. At a seamless boundary Media3 does
 * not trim the Opus packet that straddles the cut, so up to one packet (about
 * 20 ms) may be repeated or cut.
 */
private fun AndroidPlaybackSegment?.clipping(): MediaItem.ClippingConfiguration {
    if (this == null) return MediaItem.ClippingConfiguration.UNSET
    return MediaItem.ClippingConfiguration.Builder()
        .setStartPositionMs(startMs)
        .setEndPositionMs(endMs ?: C.TIME_END_OF_SOURCE)
        .setStartsAtKeyFrame(true)
        .build()
}

/** A small access-ordered map that forgets its least recently used entry. */
private class LruMap<K, V> : LinkedHashMap<K, V>(REMEMBERED_TRACKS, 0.75f, true) {
    override fun removeEldestEntry(eldest: MutableMap.MutableEntry<K, V>): Boolean =
        size > REMEMBERED_TRACKS
}
