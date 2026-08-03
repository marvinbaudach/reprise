package de.reprise.spike

fun main() {
    restoringLibraryLoadsAllThreeTabsThroughTheCorePort()
    titleSearchDelegatesTheLiteralTextToTheCorePort()
    openingAnAlbumUsesItsCoreIdentityAndOrder()
    playingFromAlbumDetailUsesTheAlbumSnapshot()
}

private fun restoringLibraryLoadsAllThreeTabsThroughTheCorePort() {
    val title = testBrowseTrack("title")
    val album = testAlbum()
    val artist = testArtist()
    val port = RecordingBrowsePort(
        titleResults = mapOf("" to listOf(title)),
        albums = listOf(album),
        artists = listOf(artist),
    )

    val state = LibrarySession(port).restore()

    check(
        state == LibraryScreenState.Browse(
            titles = listOf(title),
            albums = listOf(album),
            artists = listOf(artist),
        ),
    )
    check(
        port.operations == listOf(
            "readable:content://provider/tree/Music",
            "configure:content://provider/tree/Music",
            "search:",
            "albums",
            "artists",
        ),
    )
}

private fun titleSearchDelegatesTheLiteralTextToTheCorePort() {
    val returnedByCore = testBrowseTrack("core-result")
    val port = RecordingBrowsePort(
        titleResults = mapOf(" folk " to listOf(returnedByCore)),
    )

    val tracks = LibrarySession(port).searchTitles(" folk ")

    check(tracks == listOf(returnedByCore))
    check(port.operations == listOf("search: folk "))
}

private fun openingAnAlbumUsesItsCoreIdentityAndOrder() {
    val album = testAlbum()
    val coreOrder = listOf(testBrowseTrack("disc-one"), testBrowseTrack("disc-two"))
    val port = RecordingBrowsePort(albumTracks = coreOrder)

    val detail = LibrarySession(port).openAlbum(album)

    check(detail == AlbumTrackList(album, coreOrder))
    check(port.operations == listOf("album:Kind of Blue:Miles Davis"))
}

private fun playingFromAlbumDetailUsesTheAlbumSnapshot() {
    val albumTracks = listOf(testBrowseTrack("first"), testBrowseTrack("second"))
    val detail = AlbumTrackList(testAlbum(), albumTracks)

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

private class RecordingBrowsePort(
    private var rememberedTreeUri: String? = "content://provider/tree/Music",
    private val titleResults: Map<String, List<LibraryTrack>> = emptyMap(),
    private val albums: List<LibraryAlbum> = emptyList(),
    private val artists: List<LibraryArtist> = emptyList(),
    private val albumTracks: List<LibraryTrack> = emptyList(),
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

    override fun searchTracks(text: String): List<LibraryTrack> {
        operations += "search:$text"
        return titleResults[text].orEmpty()
    }

    override fun listAlbums(): List<LibraryAlbum> {
        operations += "albums"
        return albums
    }

    override fun listArtists(): List<LibraryArtist> {
        operations += "artists"
        return artists
    }

    override fun listAlbumTracks(album: String, albumArtist: String): List<LibraryTrack> {
        operations += "album:$album:$albumArtist"
        return albumTracks
    }
}
