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
    // the fetch waits for the page to settle. Until the marked tab is loaded
    // the line keeps answering for the one that is.
    val counted = shownTab()
        .takeIf { it == BrowseTab.QUEUE || it in loadedTabs }
        ?: selectedTab
    when (counted) {
        BrowseTab.TITLES -> visibleTitles.totalCountLabel("title", "titles")
        BrowseTab.ARTISTS -> selectedAlbum?.tracks
            ?.totalCountLabel("track", "tracks")
            ?: selectedArtist?.artist?.details()
            ?: visibleArtists.totalCountLabel("artist", "artists")
        BrowseTab.QUEUE -> "Queue"
    }
}
