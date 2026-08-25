package de.reprise.spike

import androidx.compose.runtime.staticCompositionLocalOf
import uniffi.reprise_android_ffi.AndroidArtworkSize

/**
 * How many resolved cover paths one session remembers. Large enough that
 * scrolling back through a loaded window never asks twice, small enough that
 * the map stays a rounding error next to the rows it describes.
 */
private const val REMEMBERED_ARTWORK_PATHS = 512
private const val COUNT_ONLY_WINDOW_SIZE = 1L
internal const val AUTOMATIC_SCAN_INTERVAL_MS = 5 * 60 * 1_000L

internal interface LibrarySessionPort {
    fun rememberedTreeUri(): String?

    fun rememberTreeUri(treeUri: String)

    fun persistTreePermission(treeUri: String)

    fun isTreeReadable(treeUri: String): Boolean

    fun configureTree(treeUri: String)

    fun scan(report: (LibraryScreenState.Scanning) -> Unit)

    fun lastScanCompletedAtMs(): Long = 0L

    fun rememberScanCompletedAtMs(completedAtMs: Long) = Unit

    fun searchTracks(text: String, window: LibraryWindowRange): LibraryWindow<LibraryTrack>

    fun searchAlbums(text: String, window: LibraryWindowRange): LibraryWindow<LibraryAlbum>

    fun listArtists(window: LibraryWindowRange): LibraryWindow<LibraryArtist>

    fun searchArtists(text: String, window: LibraryWindowRange): LibraryWindow<LibraryArtist>

    fun listArtistAlbums(
        artist: String,
        window: LibraryWindowRange,
    ): LibraryWindow<LibraryAlbum>

    fun listArtistUntaggedTracks(
        artist: String,
        window: LibraryWindowRange,
    ): LibraryWindow<LibraryTrack>

    fun listArtistTracks(
        artist: String,
        window: LibraryWindowRange,
    ): LibraryWindow<LibraryTrack>

    fun listAlbumTracks(
        album: String,
        albumArtist: String,
        window: LibraryWindowRange,
    ): LibraryWindow<LibraryTrack>

    fun albumTrackIds(album: String, albumArtist: String): List<Long>

    fun trackById(trackId: Long): LibraryTrack?

    fun artworkFor(trackUri: String, size: AndroidArtworkSize): String?

    fun artistPortraitCached(name: String, size: AndroidArtworkSize): String?

    fun artistPortraitFetched(name: String, size: AndroidArtworkSize): String?

    fun artistsMissingPortraits(limit: UInt): List<String>

    fun setFavourite(trackId: Long, favourite: Boolean)
}

private data class ArtworkCacheKey(
    val trackUri: String,
    val size: AndroidArtworkSize,
)

internal class LibrarySession(
    private val port: LibrarySessionPort,
    private val startPortraitPrefetch: () -> Unit = {},
    private val nowMillis: () -> Long = System::currentTimeMillis,
    private val scanMonitor: Any = Any(),
) {
    private val artworkPaths =
        object : LinkedHashMap<ArtworkCacheKey, String?>(64, 0.75f, true) {
        override fun removeEldestEntry(
            eldest: MutableMap.MutableEntry<ArtworkCacheKey, String?>,
        ): Boolean =
            size > REMEMBERED_ARTWORK_PATHS
    }

    /**
     * Bumped whenever [artworkPaths] is dropped. Resolving happens outside the
     * monitor, so a cover that was already in flight when a rescan cleared the
     * map would otherwise write its now-stale answer back into the fresh one.
     * Guarded by the [artworkPaths] monitor like the map itself.
     */
    private var artworkGeneration = 0

    fun restore(selectedTab: BrowseTab = BrowseTab.TITLES): LibraryScreenState {
        val treeUri = port.rememberedTreeUri() ?: return LibraryScreenState.NoFolder()
        if (!port.isTreeReadable(treeUri)) {
            return LibraryScreenState.TreeUnreadable
        }
        port.configureTree(treeUri)
        val state = browseState(selectedTab = selectedTab)
        startPortraitPrefetch()
        return state
    }

    fun chooseTree(
        treeUri: String,
        report: (LibraryScreenState.Scanning) -> Unit,
    ): LibraryScreenState = synchronized(scanMonitor) {
        port.persistTreePermission(treeUri)
        port.rememberTreeUri(treeUri)
        scanTree(treeUri, report)
    }

    fun rescan(report: (LibraryScreenState.Scanning) -> Unit): LibraryScreenState =
        synchronized(scanMonitor) {
            val treeUri = port.rememberedTreeUri()
                ?: return@synchronized LibraryScreenState.NoFolder()
            scanTree(treeUri, report)
        }

    /** Refreshes a configured library without replacing it with the scan screen. */
    fun autoScan(): LibraryScreenState.Browse? = synchronized(scanMonitor) {
        val treeUri = port.rememberedTreeUri() ?: return@synchronized null
        val now = nowMillis()
        val elapsed = now - port.lastScanCompletedAtMs()
        if (elapsed < AUTOMATIC_SCAN_INTERVAL_MS) return@synchronized null
        if (!port.isTreeReadable(treeUri)) return@synchronized null

        try {
            port.configureTree(treeUri)
            port.scan { }
        } finally {
            // A failed attempt waits for the normal interval instead of being
            // retried on every Activity resume.
            port.rememberScanCompletedAtMs(now)
        }
        clearArtworkPaths()
        val state = browseState()
        startPortraitPrefetch()
        state
    }

    fun stateAfterFailure(message: String): LibraryScreenState {
        val treeUri = port.rememberedTreeUri() ?: return LibraryScreenState.NoFolder(message)
        if (!port.isTreeReadable(treeUri)) {
            return LibraryScreenState.TreeUnreadable
        }
        return runCatching {
            port.configureTree(treeUri)
            browseState(message)
        }.getOrElse {
            LibraryScreenState.Browse(
                titles = LibraryWindow.empty(),
                artists = LibraryWindow.empty(),
                message = message,
                folderUri = treeUri,
            )
        }
    }

    fun searchTitles(
        text: String,
        window: LibraryWindowRange = firstLibraryWindow(),
    ): LibraryWindow<LibraryTrack> = port.searchTracks(text, window)

    fun searchAlbums(
        text: String,
        window: LibraryWindowRange,
    ): LibraryWindow<LibraryAlbum> = port.searchAlbums(text, window)

    fun listArtists(window: LibraryWindowRange): LibraryWindow<LibraryArtist> =
        port.listArtists(window)

    fun searchArtists(
        text: String,
        window: LibraryWindowRange,
    ): LibraryWindow<LibraryArtist> = port.searchArtists(text, window)

    fun listArtistAlbums(
        artist: LibraryArtist,
        window: LibraryWindowRange,
    ): LibraryWindow<LibraryAlbum> = port.listArtistAlbums(artist.name, window)

    fun listArtistUntaggedTracks(
        artist: LibraryArtist,
        window: LibraryWindowRange,
    ): LibraryWindow<LibraryTrack> = port.listArtistUntaggedTracks(artist.name, window)

    fun openAlbum(album: LibraryAlbum): AlbumTrackList = AlbumTrackList(
        album = album,
        tracks = listAlbumTracks(album, firstLibraryWindow()),
    )

    fun openArtist(artist: LibraryArtist): ArtistTrackList = ArtistTrackList(
        artist = artist,
        albums = listArtistAlbums(artist, firstLibraryWindow()),
        untaggedTracks = listArtistUntaggedTracks(artist, firstLibraryWindow()),
    )

    fun listAlbumTracks(
        album: LibraryAlbum,
        window: LibraryWindowRange,
    ): LibraryWindow<LibraryTrack> = port.listAlbumTracks(album.title, album.artist, window)

    fun albumTrackIds(album: LibraryAlbum): List<Long> =
        port.albumTrackIds(album.title, album.artist)

    fun listArtistTracks(
        artist: LibraryArtist,
        window: LibraryWindowRange,
    ): LibraryWindow<LibraryTrack> = port.listArtistTracks(artist.name, window)

    fun trackById(trackId: Long): LibraryTrack? = port.trackById(trackId)

    fun setFavourite(trackId: Long, favourite: Boolean) {
        port.setFavourite(trackId, favourite)
    }

    /**
     * The cached cover for one track, asked of the core at most once per track.
     * Resolving reads the file's tags through the document provider, which is
     * far too expensive to repeat every time a row scrolls back into view.
     *
     * The monitor covers the map accesses only, never the resolve itself: a tag
     * read plus thumbnail generation is slow enough that holding it would make
     * [rescan] wait for a cover it is about to throw away.
     */
    fun artworkFor(
        trackUri: String,
        size: AndroidArtworkSize = AndroidArtworkSize.LIST,
    ): String? {
        val key = ArtworkCacheKey(trackUri, size)
        val startedInGeneration = synchronized(artworkPaths) {
            if (artworkPaths.containsKey(key)) {
                return artworkPaths[key]
            }
            artworkGeneration
        }
        val path = port.artworkFor(trackUri, size)
        synchronized(artworkPaths) {
            if (artworkGeneration == startedInGeneration) {
                artworkPaths[key] = path
            }
        }
        return path
    }

    fun artistPortraitCached(name: String, size: AndroidArtworkSize): String? =
        port.artistPortraitCached(name, size)

    fun artistPortraitFetched(name: String, size: AndroidArtworkSize): String? =
        port.artistPortraitFetched(name, size)

    private fun scanTree(
        treeUri: String,
        report: (LibraryScreenState.Scanning) -> Unit,
    ): LibraryScreenState {
        if (!port.isTreeReadable(treeUri)) {
            return LibraryScreenState.TreeUnreadable
        }
        port.configureTree(treeUri)
        report(LibraryScreenState.Scanning())
        port.scan(report)
        port.rememberScanCompletedAtMs(nowMillis())
        // The files behind those paths may have just changed underneath us.
        clearArtworkPaths()
        val state = browseState()
        startPortraitPrefetch()
        return state
    }

    private fun clearArtworkPaths() {
        synchronized(artworkPaths) {
            artworkPaths.clear()
            artworkGeneration++
        }
    }

    private fun browseState(
        message: String? = null,
        selectedTab: BrowseTab? = null,
    ): LibraryScreenState.Browse =
        LibraryScreenState.Browse(
            titles = if (selectedTab == null || selectedTab == BrowseTab.TITLES) {
                port.searchTracks("", firstLibraryWindow())
            } else {
                port.searchTracks("", countOnlyLibraryWindow()).withoutRows()
            },
            artists = if (selectedTab == null || selectedTab == BrowseTab.ARTISTS) {
                port.listArtists(firstLibraryWindow())
            } else {
                port.listArtists(countOnlyLibraryWindow()).withoutRows()
            },
            albumCount = port.searchAlbums("", countOnlyLibraryWindow()).total,
            message = message,
            folderUri = port.rememberedTreeUri(),
            loadedTabs = selectedTab?.let(::setOf) ?: BrowseTab.entries.toSet(),
        )
}

private fun countOnlyLibraryWindow() =
    LibraryWindowRange(offset = 0, limit = COUNT_ONLY_WINDOW_SIZE)

private fun <T> LibraryWindow<T>.withoutRows() = copy(rows = emptyList(), hasMore = false)

/** The unwindowed album identity query used only by whole-album actions. */
internal val LocalAlbumTrackIds =
    staticCompositionLocalOf<(LibraryAlbum) -> List<Long>> {
        { throw IllegalStateException("library is not connected") }
    }
