package de.reprise.spike

fun main() {
    theSameLoadedWindowRequestsItsContinuationOnlyOnce()
    appendingAContinuationKeepsTheExactTotalAndOrder()
    theVisibleCountDistinguishesALoadedWindowFromTheWholeLibrary()
    restoringLibraryLoadsAllThreeTabsThroughTheCorePort()
    titleSearchDelegatesTheLiteralTextToTheCorePort()
    openingAnAlbumUsesItsCoreIdentityAndOrder()
    playingFromAlbumDetailUsesTheAlbumSnapshot()
}

private fun theSameLoadedWindowRequestsItsContinuationOnlyOnce() {
    val rows = (1..500).map { rank -> testBrowseTrack("title-$rank") }
    val window = LibraryWindow(
        total = 1_824,
        rows = rows,
        hasMore = true,
    )

    val firstRequest = window.nextRequest(lastRequestedOffset = null)

    check(firstRequest == LibraryWindowRange(offset = 500, limit = 200))
    check(window.nextRequest(lastRequestedOffset = firstRequest.offset) == null)
}

private fun appendingAContinuationKeepsTheExactTotalAndOrder() {
    val firstRows = listOf(testBrowseTrack("first"), testBrowseTrack("second"))
    val finalRow = testBrowseTrack("third")
    val first = LibraryWindow(total = 3, rows = firstRows, hasMore = true)
    val continuation = LibraryWindow(total = 3, rows = listOf(finalRow), hasMore = false)

    val complete = first.append(continuation)

    check(complete == LibraryWindow(total = 3, rows = firstRows + finalRow, hasMore = false))
    check(complete.nextRequest(lastRequestedOffset = 2) == null)
}

private fun theVisibleCountDistinguishesALoadedWindowFromTheWholeLibrary() {
    val window = LibraryWindow(
        total = 1_824,
        rows = (1..500).map { rank -> testBrowseTrack("title-$rank") },
        hasMore = true,
    )

    check(window.visibleCountLabel("title", "titles") == "500 of 1824 titles loaded")
}

private fun restoringLibraryLoadsAllThreeTabsThroughTheCorePort() {
    val title = testBrowseTrack("title")
    val album = testAlbum()
    val artist = testArtist()
    val titleWindow = LibraryWindow(total = 1_824, rows = listOf(title), hasMore = true)
    val port = RecordingBrowsePort(
        titleResults = mapOf("" to titleWindow),
        albums = completeWindow(listOf(album)),
        artists = completeWindow(listOf(artist)),
    )

    val state = LibrarySession(port).restore()

    check(
        state == LibraryScreenState.Browse(
            titles = titleWindow,
            albums = completeWindow(listOf(album)),
            artists = completeWindow(listOf(artist)),
        ),
    )
    check(
        port.operations == listOf(
            "readable:content://provider/tree/Music",
            "configure:content://provider/tree/Music",
            "search::0:200",
            "albums:0:200",
            "artists:0:200",
        ),
    )
}

private fun titleSearchDelegatesTheLiteralTextToTheCorePort() {
    val returnedByCore = testBrowseTrack("core-result")
    val returnedWindow = completeWindow(listOf(returnedByCore))
    val port = RecordingBrowsePort(
        titleResults = mapOf(" folk " to returnedWindow),
    )

    val tracks = LibrarySession(port).searchTitles(" folk ")

    check(tracks == returnedWindow)
    check(port.operations == listOf("search: folk :0:200"))
}

private fun openingAnAlbumUsesItsCoreIdentityAndOrder() {
    val album = testAlbum()
    val coreOrder = listOf(testBrowseTrack("disc-one"), testBrowseTrack("disc-two"))
    val port = RecordingBrowsePort(albumTracks = completeWindow(coreOrder))

    val detail = LibrarySession(port).openAlbum(album)

    check(detail == AlbumTrackList(album, completeWindow(coreOrder)))
    check(port.operations == listOf("album:Kind of Blue:Miles Davis:0:200"))
}

private fun playingFromAlbumDetailUsesTheAlbumSnapshot() {
    val albumTracks = listOf(testBrowseTrack("first"), testBrowseTrack("second"))
    val detail = AlbumTrackList(testAlbum(), completeWindow(albumTracks))

    val selection = detail.playbackSelection(1)

    check(selection == PlaybackSelection(albumTracks, 1))
    check(selection.tracks[selection.startIndex].title == "second")
}

private fun testBrowseTrack(title: String) = LibraryTrack(
    uri = "content://provider/document/$title.flac",
    title = title,
    artist = "Miles Davis",
    album = "Kind of Blue",
    durationMs = 1_000,
)

private fun testAlbum() = LibraryAlbum(
    title = "Kind of Blue",
    artist = "Miles Davis",
    representativeUri = "content://provider/document/so-what.flac",
    trackCount = 5,
    year = 1959,
    totalDurationMs = 2_500_000,
)

private fun testArtist() = LibraryArtist(
    name = "Miles Davis",
    trackCount = 5,
    albumCount = 1,
    representativeUri = "content://provider/document/so-what.flac",
)

private fun <T> completeWindow(rows: List<T>) = LibraryWindow(
    total = rows.size.toLong(),
    rows = rows,
    hasMore = false,
)

private class RecordingBrowsePort(
    private var rememberedTreeUri: String? = "content://provider/tree/Music",
    private val titleResults: Map<String, LibraryWindow<LibraryTrack>> = emptyMap(),
    private val albums: LibraryWindow<LibraryAlbum> = completeWindow(emptyList()),
    private val artists: LibraryWindow<LibraryArtist> = completeWindow(emptyList()),
    private val albumTracks: LibraryWindow<LibraryTrack> = completeWindow(emptyList()),
) : LibrarySessionPort {
    val operations = mutableListOf<String>()

    override fun rememberedTreeUri(): String? = rememberedTreeUri

    override fun rememberTreeUri(treeUri: String) {
        rememberedTreeUri = treeUri
    }

    override fun persistReadPermission(treeUri: String) = Unit

    override fun isTreeReadable(treeUri: String): Boolean {
        operations += "readable:$treeUri"
        return true
    }

    override fun configureTree(treeUri: String) {
        operations += "configure:$treeUri"
    }

    override fun scan(report: (LibraryScreenState.Scanning) -> Unit) = Unit

    override fun searchTracks(
        text: String,
        window: LibraryWindowRange,
    ): LibraryWindow<LibraryTrack> {
        operations += "search:$text:${window.offset}:${window.limit}"
        return titleResults[text] ?: completeWindow(emptyList())
    }

    override fun listAlbums(window: LibraryWindowRange): LibraryWindow<LibraryAlbum> {
        operations += "albums:${window.offset}:${window.limit}"
        return albums
    }

    override fun listArtists(window: LibraryWindowRange): LibraryWindow<LibraryArtist> {
        operations += "artists:${window.offset}:${window.limit}"
        return artists
    }

    override fun listAlbumTracks(
        album: String,
        albumArtist: String,
        window: LibraryWindowRange,
    ): LibraryWindow<LibraryTrack> {
        operations += "album:$album:$albumArtist:${window.offset}:${window.limit}"
        return albumTracks
    }
}
