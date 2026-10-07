package io.github.marvinbaudach.reprise.library

import android.net.Uri
import androidx.core.net.toUri
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata

/**
 * What the notification, the lock screen, Android Auto and the home-screen
 * widget show about a track. All four read it from the item's
 * [MediaMetadata], so this is the one place it is assembled.
 */
internal data class TrackMetadata(
    val trackId: Long,
    val title: String,
    val artist: String,
    val album: String,
    val durationMs: Long,
    val artworkUri: Uri? = null,
)

/**
 * Answers who a queued item is: by its library row when it has one, by its uri
 * otherwise. `null` is an ordinary answer: an unknown file.
 */
internal fun interface TrackMetadataResolver {
    fun resolve(key: PlaybackKey): TrackMetadata?

    companion object {
        val None = TrackMetadataResolver { null }
    }
}

internal fun TrackMetadata.toMediaMetadata(): MediaMetadata = MediaMetadata.Builder()
    .setTitle(title.ifBlank { null })
    .setDisplayTitle(title.ifBlank { null })
    .setArtist(artist.ifBlank { null })
    .setAlbumTitle(album.ifBlank { null })
    .setDurationMs(durationMs.takeIf { it > 0 })
    .setArtworkUri(artworkUri)
    .setMediaType(MediaMetadata.MEDIA_TYPE_MUSIC)
    .setIsBrowsable(false)
    .setIsPlayable(true)
    .build()

/**
 * The item Media3 plays for [uri]. A track the library cannot name still plays:
 * it keeps the bare uri as its id and carries no metadata. [mediaId] overrides
 * the track id when a browse tree listed the track under another one.
 */
internal fun playbackMediaItem(uri: String, metadata: TrackMetadata?, mediaId: String? = null): MediaItem {
    val builder = MediaItem.Builder().setUri(uri.toUri())
    if (metadata != null) {
        builder.setMediaId(mediaId ?: metadata.trackId.toString())
        builder.setMediaMetadata(metadata.toMediaMetadata())
    }
    return builder.build()
}
