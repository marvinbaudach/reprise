package io.github.marvinbaudach.reprise.library

import android.net.Uri
import android.util.Log
import androidx.media3.common.MediaItem

private const val TAG = "RepriseItems"

/** How many tracks' metadata and covers are remembered; a queue's worth of neighbours. */
private const val REMEMBERED_TRACKS = 64

/**
 * Builds the [androidx.media3.common.MediaItem] the player plays for a uri, and
 * remembers what it learned about each uri.
 *
 * What the library says about a track and where its cover lives cost a blocking
 * read each, but they do not change while a track is queued. Remembering them
 * means an item built a second time (a track tapped again, repeat-one, the
 * gapless next item, a notification skip) is complete at once and asks the
 * library nothing.
 *
 * Safe to call from any thread: the player's own thread builds items while the
 * Core's and the cover loader's threads teach it.
 */
internal class PlaybackItems(
    private val metadata: TrackMetadataResolver,
    /** The id the browse tree lists a track under, so a browser can mark the playing row. */
    private val mediaIdOf: (trackId: Long) -> String? = { null },
) {
    private val tracks = LruMap<String, TrackMetadata>()
    private val covers = LruMap<String, Uri>()

    fun isKnown(uri: String): Boolean = synchronized(this) { tracks.containsKey(uri) }

    /** Asks the library about [uri] and remembers the answer; blocks, so never on the player's thread. */
    fun resolve(uri: String): Boolean {
        val track = try {
            metadata.resolve(uri)
        } catch (error: Exception) {
            // Metadata is decoration: a library that cannot answer must not
            // stop the music, only leave the notification without a title.
            Log.w(TAG, "Could not read the metadata for a playing track", error)
            null
        } ?: return false
        synchronized(this) { tracks[uri] = track }
        return true
    }

    /** Remembers [cover] for [uri]; a cover that is already known is kept. */
    fun rememberCover(uri: String, cover: Uri) {
        synchronized(this) { if (!covers.containsKey(uri)) covers[uri] = cover }
    }

    /** The item for [uri] from what is remembered; a uri nothing is known about still plays, bare. */
    fun build(uri: String): MediaItem {
        val (track, cover) = synchronized(this) { tracks[uri] to covers[uri] }
        return playbackMediaItem(uri, track?.copy(artworkUri = cover), track?.let { mediaIdOf(it.trackId) })
    }
}

/** A small access-ordered map that forgets its least recently used entry. */
private class LruMap<K, V> : LinkedHashMap<K, V>(REMEMBERED_TRACKS, 0.75f, true) {
    override fun removeEldestEntry(eldest: MutableMap.MutableEntry<K, V>): Boolean =
        size > REMEMBERED_TRACKS
}
