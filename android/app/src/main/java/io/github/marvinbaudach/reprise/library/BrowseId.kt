package io.github.marvinbaudach.reprise.library

import java.net.URLDecoder
import java.net.URLEncoder

/**
 * The address of one node in the media browse tree.
 *
 * A media id is the only thing a head unit hands back, so it has to say where
 * a node lives. Album and artist names are arbitrary text, so each part is
 * URL-encoded, which also escapes the `:` that separates the parts.
 *
 * A [Track] carries its container: tapping a song in an album must queue that
 * album, tapping it under "Recently played" must queue that list. Without the
 * container a leaf would be a queue of one. It carries its position in that
 * container too, because a playlist may hold the same song twice and the id of
 * the song alone cannot say which of the two was tapped.
 */
internal sealed interface BrowseId {
    val mediaId: String

    data object Root : BrowseId {
        override val mediaId = "root"
    }

    data object RecentlyPlayed : BrowseId {
        override val mediaId = "recent"
    }

    data object Playlists : BrowseId {
        override val mediaId = "playlists"
    }

    data object Albums : BrowseId {
        override val mediaId = "albums"
    }

    data object Artists : BrowseId {
        override val mediaId = "artists"
    }

    data class Playlist(val playlistId: Long) : BrowseId {
        override val mediaId = "playlist:$playlistId"
    }

    data class Album(val title: String, val artist: String) : BrowseId {
        override val mediaId = "album:${title.escaped()}:${artist.escaped()}"
    }

    data class Artist(val name: String) : BrowseId {
        override val mediaId = "artist:${name.escaped()}"
    }

    /** A playable song, reached through [container] and listed at [position] in it. */
    data class Track(val container: BrowseId, val trackId: Long, val position: Int) : BrowseId {
        override val mediaId = "track:${container.mediaId.escaped()}:$trackId:$position"
    }

    companion object {
        /** `null` for an id this tree never produced. */
        fun parse(mediaId: String): BrowseId? {
            val parts = mediaId.split(':')
            return when (parts[0]) {
                "root" -> Root.takeIf { parts.size == 1 }
                "recent" -> RecentlyPlayed.takeIf { parts.size == 1 }
                "playlists" -> Playlists.takeIf { parts.size == 1 }
                "albums" -> Albums.takeIf { parts.size == 1 }
                "artists" -> Artists.takeIf { parts.size == 1 }
                "playlist" -> parts.longAt(1)?.takeIf { parts.size == 2 }?.let(::Playlist)
                "album" -> if (parts.size == 3) Album(parts[1].unescaped(), parts[2].unescaped()) else null
                "artist" -> if (parts.size == 2) Artist(parts[1].unescaped()) else null
                "track" -> {
                    val container = if (parts.size == 4) parse(parts[1].unescaped()) else null
                    val trackId = parts.longAt(2)
                    val position = parts.getOrNull(3)?.toIntOrNull()?.takeIf { it >= 0 }
                    if (container != null && container !is Track && trackId != null && position != null) {
                        Track(container, trackId, position)
                    } else {
                        null
                    }
                }
                else -> null
            }
        }
    }
}

private fun List<String>.longAt(index: Int): Long? = getOrNull(index)?.toLongOrNull()

private fun String.escaped(): String = URLEncoder.encode(this, Charsets.UTF_8)

private fun String.unescaped(): String = URLDecoder.decode(this, Charsets.UTF_8)
