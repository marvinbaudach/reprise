package io.github.marvinbaudach.reprise

/**
 * The count line under the tab bar, as a function the caller reads where it is
 * drawn: the tab the bar marks can change mid-swipe, and only the line should
 * be invalidated by it.
 */
internal fun browseSummary(
    shownTab: () -> BrowseTab,
    loadedTabs: Set<BrowseTab>,
    selectedTab: BrowseTab,
    visibleTitles: LibraryWindow<LibraryTrack>,
    selectedAlbum: AlbumTrackList?,
    selectedArtist: ArtistTrackList?,
    visibleArtists: LibraryWindow<LibraryArtist>,
): () -> String = {
    // The bar may already mark a tab whose window has not been fetched yet:
    // the fetch waits for the page to settle, and counting an unfetched
    // window prints a nought that reads as an answer — "0 of 65 artists"
    // where the truth is "not asked yet". Until the marked tab is loaded
    // the line keeps answering for the one that is.
    val counted = shownTab()
        .takeIf { it == BrowseTab.QUEUE || it in loadedTabs }
        ?: selectedTab
    when (counted) {
        BrowseTab.TITLES -> visibleTitles.visibleCountLabel("title", "titles")
        BrowseTab.ARTISTS -> selectedAlbum?.tracks
            ?.visibleCountLabel("track", "tracks")
            ?: selectedArtist?.let { detail ->
                val albums = detail.albums.total
                val otherTitles = detail.untaggedTracks.total
                "$albums ${if (albums == 1L) "album" else "albums"} · " +
                    "$otherTitles ${if (otherTitles == 1L) "other title" else "other titles"}"
            }
            ?: visibleArtists.visibleCountLabel("artist", "artists")
        BrowseTab.QUEUE -> "Queue"
    }
}
