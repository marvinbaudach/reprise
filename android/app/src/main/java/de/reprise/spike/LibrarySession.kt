package de.reprise.spike

internal interface LibrarySessionPort {
    fun rememberedTreeUri(): String?

    fun rememberTreeUri(treeUri: String)

    fun persistReadPermission(treeUri: String)

    fun isTreeReadable(treeUri: String): Boolean

    fun configureTree(treeUri: String)

    fun scan(report: (LibraryScreenState.Scanning) -> Unit)

    fun searchTracks(text: String): List<LibraryTrack>

    fun listAlbums(): List<LibraryAlbum>

    fun listArtists(): List<LibraryArtist>

    fun listAlbumTracks(album: String, albumArtist: String): List<LibraryTrack>
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
            LibraryScreenState.Browse(emptyList(), emptyList(), emptyList(), message)
        }
    }

    fun searchTitles(text: String): List<LibraryTrack> = port.searchTracks(text)

    fun openAlbum(album: LibraryAlbum): AlbumTrackList = AlbumTrackList(
        album = album,
        tracks = port.listAlbumTracks(album.title, album.artist),
    )

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
            titles = port.searchTracks(""),
            albums = port.listAlbums(),
            artists = port.listArtists(),
            message = message,
        )
}
