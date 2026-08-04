package de.reprise.spike

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.reprise_android_ffi.AndroidPlaybackState

class BrowseSurfaceTest {
@Test
fun theSameLoadedWindowRequestsItsContinuationOnlyOnce() {
    val rows = (1..500).map { rank -> testBrowseTrack("title-$rank") }
    val window = LibraryWindow(
        total = 1_824,
        rows = rows,
        hasMore = true,
    )

    val firstRequest = window.nextRequest(lastRequestedOffset = null)

    assertEquals(LibraryWindowRange(offset = 500, limit = 200), firstRequest)
    assertNull(window.nextRequest(lastRequestedOffset = firstRequest?.offset))
}

@Test
fun redesignedTrackListKeepsOneContinuationAtItsVisibleEnd() {
    val rows = (1..500).map { rank -> testBrowseTrack("title-$rank") }
    val window = LibraryWindow(
        total = 1_824,
        rows = rows,
        hasMore = true,
    )

    val content = trackListContent(window, lastRequestedOffset = null)
    val repeated = trackListContent(window, lastRequestedOffset = 500)

    assertEquals(501, content.size)
    assertEquals(TrackListContent.Row(index = 499, track = rows.last()), content[499])
    assertEquals(
        TrackListContent.Continuation(LibraryWindowRange(offset = 500, limit = 200)),
        content.last(),
    )
    assertEquals(1, content.count { it is TrackListContent.Continuation })
    assertEquals(500, repeated.size)
    assertEquals(0, repeated.count { it is TrackListContent.Continuation })
}

@Test
fun libraryFrameUsesTheExactTwoAMetricsAndOnlyBackedDestination() {
    assertEquals(
        LibraryFrameMetrics(
            topAppBarHeightDp = 64,
            filterChipHeightDp = 32,
            trackRowHeightDp = 72,
            trackCoverSizeDp = 56,
            miniPlayerHeightDp = 72,
            navigationBarHeightDp = 80,
        ),
        libraryFrameMetrics,
    )
    assertEquals(listOf(LibraryDestination.LIBRARY), libraryDestinations)
}

@Test
fun currentRowKeepsItsTintWhileOnlyPlayingAnimatesTheFourBars() {
    val rows = listOf(testBrowseTrack("first"), testBrowseTrack("second"))
    val selection = PlaybackSelection(rows, startIndex = 1)
    val playing = PlaybackUiState(
        ready = true,
        state = AndroidPlaybackState.PLAYING,
        currentIndex = 1,
    )
    val paused = playing.copy(state = AndroidPlaybackState.PAUSED)

    assertEquals(rows[1], selection.currentTrack(playing))
    assertEquals(
        TrackPlaybackPresentation(isCurrent = true, animateBars = true),
        rows[1].playbackPresentation(selection, playing),
    )
    assertEquals(
        TrackPlaybackPresentation(isCurrent = true, animateBars = false),
        rows[1].playbackPresentation(selection, paused),
    )
    assertEquals(false, rows[0].playbackPresentation(selection, playing).isCurrent)
    assertTrue(rows[1].playbackPresentation(selection, playing).animateBars)
}

@Test
fun appendingAContinuationKeepsTheExactTotalAndOrder() {
    val firstRows = listOf(testBrowseTrack("first"), testBrowseTrack("second"))
    val finalRow = testBrowseTrack("third")
    val first = LibraryWindow(total = 3, rows = firstRows, hasMore = true)
    val continuation = LibraryWindow(total = 3, rows = listOf(finalRow), hasMore = false)

    val complete = first.append(continuation)

    assertEquals(LibraryWindow(total = 3, rows = firstRows + finalRow, hasMore = false), complete)
    assertNull(complete.nextRequest(lastRequestedOffset = 2))
}

@Test
fun theVisibleCountDistinguishesALoadedWindowFromTheWholeLibrary() {
    val window = LibraryWindow(
        total = 1_824,
        rows = (1..500).map { rank -> testBrowseTrack("title-$rank") },
        hasMore = true,
    )

    assertEquals("500 of 1824 titles loaded", window.visibleCountLabel("title", "titles"))
}

@Test
fun restoringLibraryLoadsAllThreeTabsThroughTheCorePort() {
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

    assertEquals(
        LibraryScreenState.Browse(
            titles = titleWindow,
            albums = completeWindow(listOf(album)),
            artists = completeWindow(listOf(artist)),
        ),
        state,
    )
    assertEquals(
        listOf(
            "readable:content://provider/tree/Music",
            "configure:content://provider/tree/Music",
            "search::0:200",
            "albums:0:200",
            "artists:0:200",
        ),
        port.operations,
    )
}

@Test
fun titleSearchDelegatesTheLiteralTextToTheCorePort() {
    val returnedByCore = testBrowseTrack("core-result")
    val returnedWindow = completeWindow(listOf(returnedByCore))
    val port = RecordingBrowsePort(
        titleResults = mapOf(" folk " to returnedWindow),
    )

    val tracks = LibrarySession(port).searchTitles(" folk ")

    assertEquals(returnedWindow, tracks)
    assertEquals(listOf("search: folk :0:200"), port.operations)
}

@Test
fun openingAnAlbumUsesItsCoreIdentityAndOrder() {
    val album = testAlbum()
    val coreOrder = listOf(testBrowseTrack("disc-one"), testBrowseTrack("disc-two"))
    val port = RecordingBrowsePort(albumTracks = completeWindow(coreOrder))

    val detail = LibrarySession(port).openAlbum(album)

    assertEquals(AlbumTrackList(album, completeWindow(coreOrder)), detail)
    assertEquals(listOf("album:Kind of Blue:Miles Davis:0:200"), port.operations)
}

@Test
fun playingFromAlbumDetailUsesTheAlbumSnapshot() {
    val albumTracks = listOf(testBrowseTrack("first"), testBrowseTrack("second"))
    val detail = AlbumTrackList(testAlbum(), completeWindow(albumTracks))

    val selection = detail.playbackSelection(1)

    assertEquals(PlaybackSelection(albumTracks, 1), selection)
    assertEquals("second", selection.tracks[selection.startIndex].title)
}
}

private fun testBrowseTrack(title: String) = LibraryTrack(
    uri = "content://provider/document/$title.flac",
    title = title,
    artist = "Miles Davis",
    album = "Kind of Blue",
    durationMs = 1_000,
    playCount = 27,
    rating = 4,
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
