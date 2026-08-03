package de.reprise.spike

fun main() {
    unknownTotalUsesIndeterminateProgress()
    knownTotalUsesHonestProgressFraction()
    rememberedReadableTreeListsCatalogWithoutScanning()
    rememberedUnreadableTreeDoesNotTouchCatalog()
    choosingTreePersistsGrantAndPreferenceBeforeScanning()
    rescanUsesRememberedTreeWithoutChoosingAgain()
}

private fun unknownTotalUsesIndeterminateProgress() {
    val scanning = LibraryScreenState.Scanning(processed = 1u, total = null)

    check(scanning.progressPresentation() == ScanProgressPresentation.Indeterminate)
}

private fun knownTotalUsesHonestProgressFraction() {
    val scanning = LibraryScreenState.Scanning(processed = 1u, total = 4u)

    check(scanning.progressPresentation() == ScanProgressPresentation.Determinate(0.25f))
}

private fun rememberedReadableTreeListsCatalogWithoutScanning() {
    val port = RecordingLibrarySessionPort(
        rememberedTreeUri = "content://provider/tree/Music",
        readable = true,
        tracks = listOf(testTrack()),
    )

    val state = LibrarySession(port).restore()

    check(state == LibraryScreenState.TrackList(listOf(testTrack())))
    check(port.configuredUris == listOf("content://provider/tree/Music"))
    check(port.listCalls == 1)
    check(port.scanCalls == 0)
}

private fun rememberedUnreadableTreeDoesNotTouchCatalog() {
    val port = RecordingLibrarySessionPort(
        rememberedTreeUri = "content://provider/tree/Music",
        readable = false,
        tracks = listOf(testTrack()),
    )

    val state = LibrarySession(port).restore()

    check(state == LibraryScreenState.TreeUnreadable)
    check(port.configuredUris.isEmpty())
    check(port.listCalls == 0)
    check(port.scanCalls == 0)
}

private fun choosingTreePersistsGrantAndPreferenceBeforeScanning() {
    val port = RecordingLibrarySessionPort(
        rememberedTreeUri = null,
        readable = true,
        tracks = listOf(testTrack()),
    )
    val reports = mutableListOf<LibraryScreenState.Scanning>()

    val state = LibrarySession(port).chooseTree("content://provider/tree/Music", reports::add)

    check(state == LibraryScreenState.TrackList(listOf(testTrack())))
    check(
        port.operations == listOf(
            "persist:content://provider/tree/Music",
            "remember:content://provider/tree/Music",
            "readable:content://provider/tree/Music",
            "configure:content://provider/tree/Music",
            "scan",
            "list",
        ),
    )
    check(reports.first() == LibraryScreenState.Scanning())
}

private fun rescanUsesRememberedTreeWithoutChoosingAgain() {
    val port = RecordingLibrarySessionPort(
        rememberedTreeUri = "content://provider/tree/Music",
        readable = true,
        tracks = listOf(testTrack()),
    )
    val reports = mutableListOf<LibraryScreenState.Scanning>()

    val state = LibrarySession(port).rescan(reports::add)

    check(state == LibraryScreenState.TrackList(listOf(testTrack())))
    check(
        port.operations == listOf(
            "readable:content://provider/tree/Music",
            "configure:content://provider/tree/Music",
            "scan",
            "list",
        ),
    )
    check(reports.first() == LibraryScreenState.Scanning())
}

private fun testTrack() = LibraryTrack(
    uri = "content://provider/document/song.flac",
    title = "Song",
    artist = "Artist",
    album = "Album",
    durationMs = 1_000,
)

private class RecordingLibrarySessionPort(
    rememberedTreeUri: String?,
    private val readable: Boolean,
    private val tracks: List<LibraryTrack>,
) : LibrarySessionPort {
    private var remembered = rememberedTreeUri
    val configuredUris = mutableListOf<String>()
    val operations = mutableListOf<String>()
    var listCalls = 0
    var scanCalls = 0

    override fun rememberedTreeUri(): String? = remembered

    override fun rememberTreeUri(treeUri: String) {
        operations += "remember:$treeUri"
        remembered = treeUri
    }

    override fun persistReadPermission(treeUri: String) {
        operations += "persist:$treeUri"
    }

    override fun isTreeReadable(treeUri: String): Boolean {
        operations += "readable:$treeUri"
        return readable
    }

    override fun configureTree(treeUri: String) {
        operations += "configure:$treeUri"
        configuredUris += treeUri
    }

    override fun scan(report: (LibraryScreenState.Scanning) -> Unit) {
        operations += "scan"
        scanCalls += 1
    }

    override fun listTracks(): List<LibraryTrack> {
        operations += "list"
        listCalls += 1
        return tracks
    }
}
