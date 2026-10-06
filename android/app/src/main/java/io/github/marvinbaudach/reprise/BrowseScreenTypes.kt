package io.github.marvinbaudach.reprise

/**
 * How long the screen has to have been still before a tab nobody is looking at
 * is fetched. Long enough for the opening composition to be done with the main
 * thread, short enough to be over before a first swipe can plausibly land.
 */
internal const val NEIGHBOUR_PREFETCH_IDLE_MS = 400L

internal class BrowseReadJobs {
    var latestSearch = 0L
    var latestAlbumOpen = 0L
    var latestArtistOpen = 0L
}

internal sealed interface BrowseErrorOrigin {
    data class Tab(val tab: BrowseTab) : BrowseErrorOrigin
    data class Artist(val artist: LibraryArtist?) : BrowseErrorOrigin
    data class Album(val album: LibraryAlbum) : BrowseErrorOrigin
}

internal enum class BrowseTab(val label: String, val symbol: String) {
    TITLES("Titles", "library_music"),
    ARTISTS("Artists", "artist"),
    QUEUE("Queue", "queue_music"),
}
