package io.github.marvinbaudach.reprise

import java.util.Locale

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

/**
 * Formats a duration as `m:ss`, or `h:mm:ss` once the hour mark is reached.
 *
 * This is the Kotlin half of a rule that also exists in Rust as
 * `reprise_core::format::format_duration`. The two must agree: the same track
 * shows its length on the desktop and on the phone, and an album total or a
 * podcast episode routinely passes the hour mark. `DurationFormatTest` pins
 * this side against the very cases `crates/reprise-core/src/format.rs` asserts,
 * so a change to one contract fails the other side's test.
 *
 * `Locale.ROOT` is deliberate: the Rust side emits ASCII digits, and a
 * locale-dependent `%d` would render them differently on the phone than on the
 * desktop for the same track.
 */
internal fun formatDuration(durationMs: Long): String {
    val totalSeconds = durationMs.coerceAtLeast(0) / 1_000
    val hours = totalSeconds / 3_600
    val minutes = (totalSeconds % 3_600) / 60
    val seconds = totalSeconds % 60
    return if (hours > 0) {
        String.format(Locale.ROOT, "%d:%02d:%02d", hours, minutes, seconds)
    } else {
        String.format(Locale.ROOT, "%d:%02d", minutes, seconds)
    }
}

internal fun Throwable.browseDetail(action: String): String =
    "Could not $action: ${message ?: javaClass.simpleName}"
