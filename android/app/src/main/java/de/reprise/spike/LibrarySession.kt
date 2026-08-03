package de.reprise.spike

internal interface LibrarySessionPort {
    fun rememberedTreeUri(): String?

    fun rememberTreeUri(treeUri: String)

    fun persistReadPermission(treeUri: String)

    fun isTreeReadable(treeUri: String): Boolean

    fun configureTree(treeUri: String)

    fun scan(report: (LibraryScreenState.Scanning) -> Unit)

    fun listTracks(): List<LibraryTrack>
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
        return LibraryScreenState.TrackList(port.listTracks())
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
            LibraryScreenState.TrackList(port.listTracks(), message)
        }.getOrElse {
            LibraryScreenState.TrackList(emptyList(), message)
        }
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
        return LibraryScreenState.TrackList(port.listTracks())
    }
}
