package de.reprise.spike

internal interface LibrarySessionPort {
    fun rememberedTreeUri(): String?

    fun rememberTreeUri(treeUri: String)

    fun persistReadPermission(treeUri: String)

    fun isTreeReadable(treeUri: String): Boolean

    fun configureTree(treeUri: String)

    fun scan(report: (LibraryScreenState.Scanning) -> Unit)

    fun searchTracks(text: String, window: LibraryWindowRange): LibraryWindow<LibraryTrack>

    fun listAlbums(window: LibraryWindowRange): LibraryWindow<LibraryAlbum>

    fun listArtists(window: LibraryWindowRange): LibraryWindow<LibraryArtist>

    fun listAlbumTracks(
        album: String,
        albumArtist: String,
        window: LibraryWindowRange,
    ): LibraryWindow<LibraryTrack>
}

internal class LibrarySession(
    private val port: LibrarySessionPort,
) {
    fun restore(): LibraryScreenState {
        val treeUri = port.rememberedTreeUri() ?: return LibraryScreenState.NoFolder()
        if (!port.isTreeReadable(treeUri)) {
            return LibraryScreenState.TreeUnreadable
        }
        port.configureTree(treeUri)
        return browseState()
    }

    fun chooseTree(
        treeUri: String,
        report: (LibraryScreenState.Scanning) -> Unit,
    ): LibraryScreenState {
        port.persistReadPermission(treeUri)
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
                message,
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

    fun listAlbumTracks(
        album: LibraryAlbum,
        window: LibraryWindowRange,
    ): LibraryWindow<LibraryTrack> = port.listAlbumTracks(album.title, album.artist, window)

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
        return browseState()
    }

    private fun browseState(message: String? = null): LibraryScreenState.Browse =
        LibraryScreenState.Browse(
            titles = port.searchTracks("", firstLibraryWindow()),
            albums = port.listAlbums(firstLibraryWindow()),
            artists = port.listArtists(firstLibraryWindow()),
            message = message,
        )
}
