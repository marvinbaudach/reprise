package io.github.marvinbaudach.reprise

/**
 * The short strings the library surface builds out of its rows.
 *
 * They live together because three different surfaces read them — the track
 * row, the album list, the artist list — and a duration is formatted by the
 * mini player and the Now Playing sheet as well.
 */

internal fun LibraryTrack.details(): String =
    listOf(artist, album).filter(String::isNotBlank).joinToString(" • ").ifBlank {
        "Unknown artist"
    }

internal fun LibraryAlbum.details(): String = buildList {
    add(artist.ifBlank { "Unknown artist" })
    year?.let { add(it.toString()) }
    add("$trackCount tracks")
}.joinToString(" • ")

internal fun LibraryArtist.details(): String = "$albumCount albums • $trackCount tracks"

internal fun formatDuration(durationMs: Long): String {
    val totalSeconds = durationMs.coerceAtLeast(0) / 1_000
    return "%d:%02d".format(totalSeconds / 60, totalSeconds % 60)
}

internal fun Throwable.browseDetail(action: String): String =
    "Could not $action: ${message ?: javaClass.simpleName}"
