package io.github.marvinbaudach.reprise

import java.util.concurrent.atomic.AtomicInteger

/**
 * The most rows one read asks for.
 *
 * Mirrors `MAX_WINDOW_LIMIT` in `crates/reprise-core/src/queries/mod.rs`, which
 * clamps every window to it; asking for more would only be answered short.
 */
private const val RELOAD_CHUNK_LIMIT = 500L

/**
 * The library after a deletion: the fresh catalog, and — when the screen had
 * windows open — those windows read again to the depth they had.
 *
 * [reloadFailure] is why [windows] is null although there was something to
 * rebuild. The fresh [state] is still good then; the screen simply starts from
 * its first window, exactly as it does after a scan.
 */
internal data class RefreshedLibrary(
    val state: LibraryScreenState.Browse,
    val windows: LoadedLibraryWindows?,
    val reloadFailure: Throwable? = null,
)

/**
 * What the screen was showing when a refresh began, and the check that the
 * rebuilt windows may still be put back.
 *
 * The rebuild runs off the main thread, so the listener can move meanwhile. A
 * page they closed must not be pushed open again, and a search they changed
 * must not be answered from the old one.
 */
internal data class RemovalRefreshBasis(
    val windows: LoadedLibraryWindows?,
    val selectedTab: BrowseTab,
) {
    fun stillDescribes(
        current: LoadedLibraryWindows?,
        currentTab: BrowseTab,
        currentSearchText: String,
    ): Boolean {
        val before = windows ?: return false
        current ?: return false
        return currentTab == selectedTab &&
            currentSearchText == before.searchText &&
            current.searchText == before.searchText &&
            current.openArtist?.artist?.name == before.openArtist?.artist?.name &&
            current.openAlbum?.album?.identity() == before.openAlbum?.album?.identity()
    }
}

/**
 * Re-reads the library after tracks were deleted and hands it to the screen.
 *
 * The read runs on [onWorker]; taking the basis and applying the result happen
 * on the main thread ([refresh]'s caller and [onMain]). Each call takes a
 * ticket, and a result whose ticket is older than one already applied is
 * dropped: two deletions in quick succession may finish their reads in either
 * order, and the older read must not overwrite the newer.
 */
internal class LibraryRemovalRefresher(
    private val session: LibrarySession,
    private val surface: MobileSurfaceViewModel,
    private val onWorker: (() -> Unit) -> Unit,
    private val onMain: (() -> Unit) -> Unit,
    private val logFailure: (String, Throwable) -> Unit,
) {
    private val tickets = AtomicInteger()
    private var applied = 0

    /** Main thread only. */
    fun refresh() {
        val ticket = tickets.incrementAndGet()
        val basis = surface.removalRefreshBasis()
        onWorker {
            runCatching { session.refreshBrowse(basis.windows) }
                .onSuccess { refreshed ->
                    onMain {
                        refreshed.reloadFailure?.let { error ->
                            logFailure("Could not restore the open lists after a deletion", error)
                        }
                        if (ticket > applied) {
                            applied = ticket
                            surface.updateLibraryAfterRemoval(refreshed, basis)
                        }
                    }
                }
                .onFailure { error ->
                    onMain { logFailure("Could not refresh the library after a deletion", error) }
                }
        }
    }
}

/**
 * Reads back every list [previous] had open, to at least the depth it had.
 *
 * Depth matters because the scroll anchor is an index into the rows that were
 * paged in: an anchor at row 450 means nothing to a list that reloaded 200.
 * A page whose content is gone is dropped rather than reopened empty.
 */
internal fun LibrarySession.rebuildWindows(
    state: LibraryScreenState.Browse,
    previous: LoadedLibraryWindows,
): LoadedLibraryWindows {
    val text = previous.searchText
    val titles = if (BrowseTab.TITLES in previous.loadedTabs) {
        readBack(previous.titles.rows.size) { range -> searchTitles(text, range) }
    } else {
        state.titles.withoutRows()
    }
    val artists = if (BrowseTab.ARTISTS in previous.loadedTabs) {
        readBack(previous.artists.rows.size) { range ->
            if (text.isBlank()) listArtists(range) else searchArtists(text, range)
        }
    } else {
        state.artists.withoutRows()
    }
    val artistPage = previous.openArtist?.let(::rebuildArtistPage)
    val albumPage = when {
        previous.openAlbum == null -> null
        previous.openArtist != null && artistPage == null -> null
        else -> rebuildAlbumPage(previous.openAlbum, artistPage)
    }
    return previous.copy(
        titles = titles,
        artists = artists,
        openAlbum = albumPage,
        openArtist = artistPage,
    )
}

private fun LibrarySession.rebuildArtistPage(old: ArtistTrackList): ArtistTrackList? {
    val albums = readBack(old.albums.rows.size) { range -> listArtistAlbums(old.artist, range) }
    val untagged = readBack(old.untaggedTracks.rows.size) { range ->
        listArtistUntaggedTracks(old.artist, range)
    }
    if (albums.total == 0L && untagged.total == 0L) {
        return null
    }
    return ArtistTrackList(freshArtist(old.artist), albums, untagged)
}

private fun LibrarySession.rebuildAlbumPage(
    old: AlbumTrackList,
    artistPage: ArtistTrackList?,
): AlbumTrackList? {
    val tracks = readBack(old.tracks.rows.size) { range -> listAlbumTracks(old.album, range) }
    if (tracks.total == 0L) {
        return null
    }
    // The header's counts are part of the album's identity to the screen, so
    // the row it now has replaces the one it was opened from.
    val album = artistPage?.albums?.rows?.firstOrNull { it.identity() == old.album.identity() }
        ?: old.album.copy(trackCount = tracks.total)
    return AlbumTrackList(album, tracks)
}

/** The artist as the list now describes them; the old row when the list cannot say. */
private fun LibrarySession.freshArtist(old: LibraryArtist): LibraryArtist {
    var offset = 0L
    while (true) {
        val page = searchArtists(old.name, LibraryWindowRange(offset, RELOAD_CHUNK_LIMIT))
        page.rows.firstOrNull { it.name == old.name }?.let { return it }
        if (!page.hasMore || page.rows.isEmpty()) {
            return old
        }
        offset += page.rows.size
    }
}

private fun <T> readBack(
    loaded: Int,
    read: (LibraryWindowRange) -> LibraryWindow<T>,
): LibraryWindow<T> {
    val wanted = maxOf(loaded.toLong(), firstLibraryWindow().limit)
    var window = read(LibraryWindowRange(0, minOf(wanted, RELOAD_CHUNK_LIMIT)))
    while (window.rows.size < wanted && window.hasMore) {
        val offset = window.rows.size.toLong()
        val next = read(LibraryWindowRange(offset, minOf(wanted - offset, RELOAD_CHUNK_LIMIT)))
        if (next.rows.isEmpty()) {
            break
        }
        window = window.append(next)
    }
    return window
}
