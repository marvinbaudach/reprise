package de.reprise.spike

import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.unit.dp
import de.reprise.spike.ui.theme.NocturneShapes
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicReference
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNotSame
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.reprise_android_ffi.AndroidArtworkSize
import uniffi.reprise_android_ffi.AndroidPlaybackState
import uniffi.reprise_android_ffi.AndroidRepeatMode

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class BrowseSurfaceTest {
    @Test
    fun seekDragOwnsTheHeadUntilRelease() {
        val initial = SeekPositionState.fromSnapshot(12_000)
        val dragging = initial.dragTo(48_000)

        assertEquals(48_000, dragging.acceptSnapshot(13_000).positionMs)
        assertTrue(dragging.isDragging)

        val released = dragging.release()
        assertEquals(13_500, released.acceptSnapshot(13_500).positionMs)
        assertEquals(false, released.isDragging)
    }

    @Test
    fun fullArtworkHasItsOwnLazyCacheEntry() {
        val track = testBrowseTrack("title")
        val port = RecordingBrowsePort()
        val session = LibrarySession(port)

        session.artworkFor(track.uri, AndroidArtworkSize.LIST)
        session.artworkFor(track.uri, AndroidArtworkSize.NOW_PLAYING)
        session.artworkFor(track.uri, AndroidArtworkSize.LIST)

        assertEquals(
            listOf(
                track.uri to AndroidArtworkSize.LIST,
                track.uri to AndroidArtworkSize.NOW_PLAYING,
            ),
            port.artworkRequests,
        )
    }

    @Test
    fun nowPlayingUsesTheMeasuredSheetAndCoverMetrics() {
        assertEquals(
            NowPlayingMetrics(
                coverSizeDp = 364,
                coverRadiusDp = 28,
                titleSizeSp = 28,
                titleLineHeightSp = 36,
                artistSizeSp = 16,
                artistLineHeightSp = 24,
                playButtonSizeDp = 80,
                playButtonRadiusDp = 28,
            ),
            nowPlayingMetrics,
        )
    }

    /**
     * The sheet clips its play button with the theme's `extraLarge`, so the
     * rounded square the frame asks for only stays one if that rung stays 28 dp.
     */
    @Test
    fun theSheetsPlayButtonRungIsTheFramesRoundedSquare() {
        assertEquals(
            RoundedCornerShape(nowPlayingMetrics.playButtonRadiusDp.dp),
            NocturneShapes.extraLarge,
        )
    }

    @Test
    fun theSeekReadoutCountsDownWhatIsLeftRatherThanUpToTheTotal() {
        assertEquals("−1:00", formatRemaining(positionMs = 80_000, durationMs = 140_000))
        assertEquals("−0:00", formatRemaining(positionMs = 140_000, durationMs = 140_000))
        assertEquals("−0:00", formatRemaining(positionMs = 200_000, durationMs = 140_000))
        assertEquals("--:--", formatRemaining(positionMs = 3_000, durationMs = 0))
    }

    /**
     * A rating that fails the same way twice is still two failures; the second
     * must restart its own dismissal rather than ride out the first one's.
     */
    @Test
    fun aRepeatedRatingFailureIsANewMessageWithItsOwnLifetime() {
        val first = TransientMessage("Could not save rating: gone")
        val second = TransientMessage("Could not save rating: gone").after(first)

        assertEquals(first.text, second.text)
        assertNotEquals(first, second)
        assertEquals(TransientMessage("Could not save rating: gone"), first.after(null))
    }

    /**
     * The default behind [LocalPlaybackControls] must not be able to pass for a
     * connected player. Every command is a no-op — and the one command that has
     * to answer answers with a failure, never with the null that means the
     * rating reached the database.
     */
    @Test
    fun theDisconnectedControlsDoNothingAndNeverClaimARatingWasSaved() {
        val controls: PlaybackControls = DisconnectedPlaybackControls

        controls.togglePause()
        controls.next()
        controls.previous()
        controls.seekTo(42_000)
        controls.setShuffle(true)
        controls.setRepeat(AndroidRepeatMode.ALL)

        var answered: String? = null
        var answers = 0
        controls.setFavourite(trackId = 830, favourite = true) { message ->
            answered = message
            answers += 1
        }

        assertEquals("Could not save rating: playback is not connected.", answered)
        assertEquals(1, answers)
    }

    @Test
    fun repeatButtonCyclesAllThreeReadableModes() {
        assertEquals(AndroidRepeatMode.ALL, cycleRepeatMode(AndroidRepeatMode.OFF))
        assertEquals(AndroidRepeatMode.ONE, cycleRepeatMode(AndroidRepeatMode.ALL))
        assertEquals(AndroidRepeatMode.OFF, cycleRepeatMode(AndroidRepeatMode.ONE))
    }

    @Test
    fun artworkResolutionIsALazyCallSeparateFromPagedTracks() {
        val track = testBrowseTrack("title")
        val port = RecordingBrowsePort(
            titleResults = mapOf("" to completeWindow(listOf(track))),
            artwork = mapOf(track.uri to "/private/cache/reprise/covers/title-168.png"),
        )
        val session = LibrarySession(port)

        session.searchTitles("")
        assertEquals(listOf("search::0:200"), port.operations)

        assertEquals(
            "/private/cache/reprise/covers/title-168.png",
            session.artworkFor(track.uri),
        )
        assertEquals(listOf("search::0:200", "artwork:${track.uri}"), port.operations)
    }

    @Test
    fun oneTrackIsResolvedOnceHoweverOftenItsRowComesBack() {
        val track = testBrowseTrack("title")
        val port = RecordingBrowsePort(
            artwork = mapOf(track.uri to "/private/cache/reprise/covers/title-168.png"),
        )
        val session = LibrarySession(port)

        repeat(3) { session.artworkFor(track.uri) }

        assertEquals(listOf("artwork:${track.uri}"), port.operations)
    }

    @Test
    fun aTrackWithoutArtworkIsAlsoAskedOnlyOnce() {
        val track = testBrowseTrack("title")
        val port = RecordingBrowsePort()
        val session = LibrarySession(port)

        assertNull(session.artworkFor(track.uri))
        assertNull(session.artworkFor(track.uri))

        assertEquals(listOf("artwork:${track.uri}"), port.operations)
    }

    @Test
    fun rescanningForgetsTheCoversItResolvedBefore() {
        val track = testBrowseTrack("title")
        val port = RecordingBrowsePort(
            artwork = mapOf(track.uri to "/private/cache/reprise/covers/title-168.png"),
        )
        val session = LibrarySession(port)
        session.artworkFor(track.uri)

        session.rescan {}
        session.artworkFor(track.uri)

        assertEquals(2, port.operations.count { it == "artwork:${track.uri}" })
    }

    /**
     * Resolving a cover no longer holds the map's monitor, so a rescan can now
     * clear the map while a resolve is in flight. The hook fires exactly at
     * that point — from the resolving call itself, which is the deterministic
     * stand-in for the background rescan thread — and the answer it produces
     * describes files the rescan has just replaced, so it must not be kept.
     */
    @Test
    fun aCoverResolvedAcrossARescanIsNotRememberedFromTheOldScan() {
        val track = testBrowseTrack("title")
        var session: LibrarySession? = null
        val port = RecordingBrowsePort(
            artwork = mapOf(track.uri to "/private/cache/reprise/covers/title-168.png"),
            whileResolvingArtwork = { session?.rescan {} },
        )
        session = LibrarySession(port)

        assertEquals(
            "/private/cache/reprise/covers/title-168.png",
            session.artworkFor(track.uri),
        )
        session.artworkFor(track.uri)

        assertEquals(2, port.operations.count { it == "artwork:${track.uri}" })
    }

    @Test
    fun recycledArtworkRequestCannotReplaceTheNewRowsImage() {
        val gate = ArtworkRequestGate()
        val oldRow = gate.begin("content://provider/document/old.flac")
        val newRow = gate.begin("content://provider/document/new.flac")

        assertFalse(gate.accepts(oldRow))
        assertTrue(gate.accepts(newRow))
        gate.invalidate(newRow)
        assertFalse(gate.accepts(newRow))
    }

    @Test
    fun artworkFinishingAfterRecyclingIsNotDeliveredToTheNewRow() {
        val callerThread = Thread.currentThread()
        val resolvingThread = AtomicReference<Thread>()
        val mainThreadWork = AtomicReference<() -> Unit>()
        val deliveryScheduled = CountDownLatch(1)
        var deliveries = 0
        val artwork = TrackArtwork(
            resolve = { _, _ ->
                resolvingThread.set(Thread.currentThread())
                throw IllegalStateException("synthetic failed artwork read")
            },
            fallback = { _, _, _ ->
                android.graphics.Bitmap.createBitmap(4, 4, android.graphics.Bitmap.Config.ARGB_8888)
            },
            cache = ArtworkCache(),
            onMainThread = { work ->
                mainThreadWork.set(work)
                deliveryScheduled.countDown()
            },
        )
        val gate = ArtworkRequestGate()
        val oldRow = gate.begin("content://provider/document/old.flac")

        try {
            artwork.load(oldRow, gate) { deliveries += 1 }
            assertTrue(deliveryScheduled.await(5, TimeUnit.SECONDS))
            assertNotSame(callerThread, resolvingThread.get())

            gate.begin("content://provider/document/new.flac")
            mainThreadWork.get().invoke()

            assertEquals(0, deliveries)
        } finally {
            artwork.shutdown()
        }
    }

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
    fun shrinkingALoadedWindowInvalidatesTheOffsetThatCouldStallItsNextPage() {
        val loadedRows = (1..400).map { rank ->
            testBrowseTrack("favourite-$rank").copy(id = rank.toLong(), rating = 5)
        }
        var window = LibraryWindow(total = 800, rows = loadedRows, hasMore = true)
        var lastRequestedOffset: Long? = 200

        loadedRows.take(200).forEach { track ->
            val removal = window.removeTrack(track.id, lastRequestedOffset)
            window = removal.window
            lastRequestedOffset = removal.lastRequestedOffset
        }

        assertEquals(200, window.rows.size)
        assertEquals(600, window.total)
        assertEquals(
            LibraryWindowRange(offset = 200, limit = 200),
            window.nextRequest(lastRequestedOffset),
        )
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
fun libraryFrameUsesTheExactTwoAMetricsAndThreeBrowseDestinations() {
    assertEquals(
        LibraryFrameMetrics(
            filterChipHeightDp = 32,
            trackRowHeightDp = 72,
            trackCoverSizeDp = 56,
            miniPlayerHeightDp = 72,
            navigationBarHeightDp = 80,
        ),
        libraryFrameMetrics,
    )
    assertEquals(
        listOf(
            BrowseTab.TITLES,
            BrowseTab.ARTISTS,
            BrowseTab.QUEUE,
        ),
        libraryDestinations,
    )
}

@Test
fun currentRowKeepsItsTintWhileOnlyPlayingAnimatesTheFourBars() {
    val rows = listOf(testBrowseTrack("first"), testBrowseTrack("second"))
    val playing = PlaybackUiState(
        ready = true,
        state = AndroidPlaybackState.PLAYING,
        currentIndex = 1,
        currentTrackId = rows[1].id,
        currentTrackUri = rows[1].uri,
    )
    val paused = playing.copy(state = AndroidPlaybackState.PAUSED)

    assertEquals(
        TrackPlaybackPresentation(isCurrent = true, animateBars = true),
        rows[1].playbackPresentation(playing),
    )
    assertEquals(
        TrackPlaybackPresentation(isCurrent = true, animateBars = false),
        rows[1].playbackPresentation(paused),
    )
    assertEquals(false, rows[0].playbackPresentation(playing).isCurrent)
    assertTrue(rows[1].playbackPresentation(playing).animateBars)
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
fun restoringLibraryLoadsTheDefaultDestinationAndOnlyCountsTheRestThroughTheCorePort() {
    val title = testBrowseTrack("title")
    val titleWindow = LibraryWindow(total = 1_824, rows = listOf(title), hasMore = true)
    val port = RecordingBrowsePort(
        titleResults = mapOf("" to titleWindow),
    )

    val state = LibrarySession(port).restore()

    assertEquals(
        LibraryScreenState.Browse(
            titles = titleWindow,
            artists = LibraryWindow.empty(),
            folderUri = "content://provider/tree/Music",
            loadedTabs = setOf(BrowseTab.TITLES),
        ),
        state,
    )
    assertEquals(
        // d568a00770 intentionally added count-only off-tab queries; this mirrors the
        // already-updated remembered-destination assertion in LibraryScreenStateTest.
        listOf(
            "readable:content://provider/tree/Music",
            "configure:content://provider/tree/Music",
            "search::0:200",
            "artists:0:1",
            "search-albums::0:1",
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
fun browseSearchesDelegateLiteralTextAndWindowToTheirPortMethods() {
    val port = RecordingBrowsePort()
    val session = LibrarySession(port)
    val window = LibraryWindowRange(offset = 200, limit = 75)

    session.searchAlbums(" slow ", window)
    session.searchArtists(" slow ", window)

    assertEquals(
        listOf(
            "search-albums: slow :200:75",
            "search-artists: slow :200:75",
        ),
        port.operations,
    )
}

@Test
fun emptyBrowseMessagesNameTheFilteredDestination() {
    assertEquals("No matching titles.", BrowseTab.TITLES.emptyMessage("slow"))
    assertEquals("No matching artists.", BrowseTab.ARTISTS.emptyMessage("slow"))
}

@Test
fun openingAnAlbumFromArtistDetailUsesItsCoreIdentityAndOrder() {
    val album = testAlbum()
    val coreOrder = listOf(testBrowseTrack("disc-one"), testBrowseTrack("disc-two"))
    val port = RecordingBrowsePort(albumTracks = completeWindow(coreOrder))
    val artistDetail = ArtistTrackList(
        artist = LibraryArtist("Miles Davis", 2, 1, "content://miles"),
        albums = completeWindow(listOf(album)),
    )

    val detail = LibrarySession(port).openAlbum(artistDetail.albums.rows.single())

    assertEquals(AlbumTrackList(album, completeWindow(coreOrder)), detail)
    assertEquals(listOf("album:Kind of Blue:Miles Davis:0:200"), port.operations)
}

}

private fun testBrowseTrack(title: String) = LibraryTrack(
    id = title.hashCode().toLong(),
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
    private val artistTracks: LibraryWindow<LibraryTrack> = completeWindow(emptyList()),
    private val albumTracks: LibraryWindow<LibraryTrack> = completeWindow(emptyList()),
    private val artwork: Map<String, String?> = emptyMap(),
    private val whileResolvingArtwork: (String) -> Unit = {},
) : LibrarySessionPort {
    val operations = mutableListOf<String>()
    val artworkRequests = mutableListOf<Pair<String, AndroidArtworkSize>>()

    override fun rememberedTreeUri(): String? = rememberedTreeUri

    override fun rememberTreeUri(treeUri: String) {
        rememberedTreeUri = treeUri
    }

    override fun persistTreePermission(treeUri: String) = Unit

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

    override fun searchAlbums(
        text: String,
        window: LibraryWindowRange,
    ): LibraryWindow<LibraryAlbum> {
        operations += "search-albums:$text:${window.offset}:${window.limit}"
        return albums
    }

    override fun listArtists(window: LibraryWindowRange): LibraryWindow<LibraryArtist> {
        operations += "artists:${window.offset}:${window.limit}"
        return artists
    }

    override fun searchArtists(
        text: String,
        window: LibraryWindowRange,
    ): LibraryWindow<LibraryArtist> {
        operations += "search-artists:$text:${window.offset}:${window.limit}"
        return artists
    }

    override fun listArtistAlbums(
        artist: String,
        window: LibraryWindowRange,
    ): LibraryWindow<LibraryAlbum> {
        operations += "artist-albums:$artist:${window.offset}:${window.limit}"
        return completeWindow(emptyList())
    }

    override fun listArtistUntaggedTracks(
        artist: String,
        window: LibraryWindowRange,
    ): LibraryWindow<LibraryTrack> {
        operations += "artist-untagged:$artist:${window.offset}:${window.limit}"
        return completeWindow(emptyList())
    }

    override fun listArtistTracks(
        artist: String,
        window: LibraryWindowRange,
    ): LibraryWindow<LibraryTrack> {
        operations += "artist:$artist:${window.offset}:${window.limit}"
        return artistTracks
    }

    override fun listAlbumTracks(
        album: String,
        albumArtist: String,
        window: LibraryWindowRange,
    ): LibraryWindow<LibraryTrack> {
        operations += "album:$album:$albumArtist:${window.offset}:${window.limit}"
        return albumTracks
    }

    override fun albumTrackIds(album: String, albumArtist: String): List<Long> =
        albumTracks.rows.map(LibraryTrack::id)

    override fun trackById(trackId: Long): LibraryTrack? =
        titleResults.values.asSequence().flatMap { it.rows }.firstOrNull { it.id == trackId }

    override fun artworkFor(trackUri: String, size: AndroidArtworkSize): String? {
        operations += "artwork:$trackUri"
        artworkRequests += trackUri to size
        whileResolvingArtwork(trackUri)
        return artwork[trackUri]
    }

    override fun artistPortraitCached(name: String, size: AndroidArtworkSize): String? = null

    override fun artistPortraitFetched(name: String, size: AndroidArtworkSize): String? = null

    override fun setFavourite(trackId: Long, favourite: Boolean) {
        operations += "rating:$trackId:${if (favourite) 5 else 0}"
    }
}
