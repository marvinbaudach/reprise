package de.reprise.spike

import uniffi.reprise_android_ffi.AndroidArtworkSize

/**
 * How many resolved cover paths one session remembers. Large enough that
 * scrolling back through a loaded window never asks twice, small enough that
 * the map stays a rounding error next to the rows it describes.
 */
private const val REMEMBERED_ARTWORK_PATHS = 512

internal interface LibrarySessionPort {
    fun rememberedTreeUri(): String?

    fun rememberTreeUri(treeUri: String)

    fun persistTreePermission(treeUri: String)

    fun isTreeReadable(treeUri: String): Boolean

    fun configureTree(treeUri: String)

    fun scan(report: (LibraryScreenState.Scanning) -> Unit)

    fun searchTracks(text: String, window: LibraryWindowRange): LibraryWindow<LibraryTrack>

    fun listAlbums(window: LibraryWindowRange): LibraryWindow<LibraryAlbum>

    fun listArtists(window: LibraryWindowRange): LibraryWindow<LibraryArtist>

    fun listArtistTracks(
        artist: String,
        window: LibraryWindowRange,
    ): LibraryWindow<LibraryTrack>

    fun listFavourites(window: LibraryWindowRange): LibraryWindow<LibraryTrack>

    fun listAlbumTracks(
        album: String,
        albumArtist: String,
        window: LibraryWindowRange,
    ): LibraryWindow<LibraryTrack>

    fun trackById(trackId: Long): LibraryTrack?

    fun artworkFor(trackUri: String, size: AndroidArtworkSize): String?

    fun setFavourite(trackId: Long, favourite: Boolean)
}

private data class ArtworkCacheKey(
    val trackUri: String,
    val size: AndroidArtworkSize,
)

internal class LibrarySession(
    private val port: LibrarySessionPort,
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
        return browseState(selectedTab = selectedTab)
    }

    fun chooseTree(
        treeUri: String,
        report: (LibraryScreenState.Scanning) -> Unit,
    ): LibraryScreenState {
        port.persistTreePermission(treeUri)
        port.rememberTreeUri(treeUri)
        return scanTree(treeUri, report)
    }

    fun rescan(report: (LibraryScreenState.Scanning) -> Unit): LibraryScreenState {
        val treeUri = port.rememberedTreeUri() ?: return LibraryScreenState.NoFolder()
        return scanTree(treeUri, report)
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
                LibraryWindow.empty(),
                LibraryWindow.empty(),
                LibraryWindow.empty(),
                LibraryWindow.empty(),
                message,
                treeUri,
            )
        }
    }

    fun searchTitles(
        text: String,
        window: LibraryWindowRange = firstLibraryWindow(),
    ): LibraryWindow<LibraryTrack> = port.searchTracks(text, window)

    fun listAlbums(window: LibraryWindowRange): LibraryWindow<LibraryAlbum> =
        port.listAlbums(window)

    fun listArtists(window: LibraryWindowRange): LibraryWindow<LibraryArtist> =
        port.listArtists(window)

    fun openAlbum(album: LibraryAlbum): AlbumTrackList = AlbumTrackList(
        album = album,
        tracks = listAlbumTracks(album, firstLibraryWindow()),
    )

    fun openArtist(artist: LibraryArtist): ArtistTrackList = ArtistTrackList(
        artist = artist,
        tracks = listArtistTracks(artist, firstLibraryWindow()),
    )

    fun listAlbumTracks(
        album: LibraryAlbum,
        window: LibraryWindowRange,
    ): LibraryWindow<LibraryTrack> = port.listAlbumTracks(album.title, album.artist, window)

    fun listArtistTracks(
        artist: LibraryArtist,
        window: LibraryWindowRange,
    ): LibraryWindow<LibraryTrack> = port.listArtistTracks(artist.name, window)

    fun listFavourites(window: LibraryWindowRange): LibraryWindow<LibraryTrack> =
        port.listFavourites(window)

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
        // The files behind those paths may have just changed underneath us.
        synchronized(artworkPaths) {
            artworkPaths.clear()
            artworkGeneration++
        }
        return browseState()
    }

    private fun browseState(
        message: String? = null,
        selectedTab: BrowseTab? = null,
    ): LibraryScreenState.Browse =
        LibraryScreenState.Browse(
            titles = if (selectedTab == null || selectedTab == BrowseTab.TITLES) {
                port.searchTracks("", firstLibraryWindow())
            } else {
                LibraryWindow.empty()
            },
            albums = if (selectedTab == null || selectedTab == BrowseTab.ALBUMS) {
                port.listAlbums(firstLibraryWindow())
            } else {
                LibraryWindow.empty()
            },
            artists = if (selectedTab == null || selectedTab == BrowseTab.ARTISTS) {
                port.listArtists(firstLibraryWindow())
            } else {
                LibraryWindow.empty()
            },
            favourites = if (selectedTab == null || selectedTab == BrowseTab.FAVOURITES) {
                port.listFavourites(firstLibraryWindow())
            } else {
                LibraryWindow.empty()
            },
            message = message,
            folderUri = port.rememberedTreeUri(),
            loadedTabs = selectedTab?.let(::setOf) ?: BrowseTab.entries.toSet(),
        )
}
