package de.reprise.spike

import uniffi.reprise_android_ffi.AndroidArtworkSize

/**
 * One list slot's claim on an artwork resolution.
 *
 * Identity decides, not the track URI: the same track can legitimately be
 * asked for twice, and the second ask must win.
 */
internal class ArtworkRequest(
    val trackUri: String,
    val size: AndroidArtworkSize,
    val title: String = "",
    val artist: String = "",
)

/**
 * Admits only the newest request a slot made.
 *
 * A row is recycled to another track while its cover is still being read, so
 * a finished read has to prove it still belongs to what the slot shows now.
 * This is the Kotlin half of the recycle token the desktop cover loader uses
 * (`reprise-gnome/src/ui/cover/cover_loader.rs`); without it a scrolled-away
 * row overwrites the image of the row that took its place.
 */
internal class ArtworkRequestGate {
    private var current: ArtworkRequest? = null

    @Synchronized
    fun begin(
        trackUri: String,
        size: AndroidArtworkSize = AndroidArtworkSize.LIST,
        title: String = "",
        artist: String = "",
    ): ArtworkRequest = ArtworkRequest(trackUri, size, title, artist).also { request ->
        current = request
    }

    @Synchronized
    fun accepts(request: ArtworkRequest): Boolean = current === request

    @Synchronized
    fun invalidate(request: ArtworkRequest) {
        if (current === request) {
            current = null
        }
    }
}
