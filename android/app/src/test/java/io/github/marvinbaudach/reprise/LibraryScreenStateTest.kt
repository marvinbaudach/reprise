package io.github.marvinbaudach.reprise

import androidx.media3.common.Player
import java.lang.reflect.Proxy
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.reprise_android_ffi.AndroidArtworkSize
import uniffi.reprise_android_ffi.AndroidPlaybackSnapshot
import uniffi.reprise_android_ffi.AndroidPlaybackState
import uniffi.reprise_android_ffi.AndroidRepeatMode

class LibraryScreenStateTest {
@Test
fun mediaSessionTransportReturnsToCore() {
    var playWhenReady = false
    val player = Proxy.newProxyInstance(
        Player::class.java.classLoader,
        arrayOf(Player::class.java),
    ) { _, method, _ ->
        when (method.name) {
            "getPlayWhenReady" -> playWhenReady
            else -> primitiveDefault(method.returnType)
        }
    } as Player
    val calls = mutableListOf<String>()
    val controlled = CoreControlledPlayer(player, object : CoreControlledPlayer.Commands {
        override fun togglePause() {
            calls += "toggle"
        }

        override fun next() {
            calls += "next"
        }

        override fun previousInQueueOrder() {
            calls += "queue-previous"
        }
    })

    controlled.play()
    playWhenReady = true
    controlled.pause()
    controlled.seekToNext()
    controlled.seekToPrevious()
    controlled.seekToPreviousMediaItem()

    assertEquals(listOf("toggle", "toggle", "next", "queue-previous", "queue-previous"), calls)
}

private fun primitiveDefault(type: Class<*>): Any? = when (type) {
    Boolean::class.javaPrimitiveType -> false
    Int::class.javaPrimitiveType -> 0
    Long::class.javaPrimitiveType -> 0L
    Float::class.javaPrimitiveType -> 0f
    Double::class.javaPrimitiveType -> 0.0
    else -> null
}

@Test
fun everyFieldTheSurfaceReadsSurvivesTheTripFromTheBridge() {
    val state = AndroidPlaybackSnapshot(
        state = AndroidPlaybackState.PLAYING,
        currentIndex = 2u,
        currentTrackId = 41,
        currentTrackUri = "content://provider/playing.flac",
        positionMs = 1_250,
        durationMs = 180_000,
        automaticAdvanceCount = 7u,
        shuffled = true,
        repeat = AndroidRepeatMode.ALL,
        error = null,
    ).toUiState()

    assertEquals(2, state.currentIndex)
    assertEquals(2, state.libraryPlayback().currentIndex)
    assertEquals(41L, state.currentTrackId)
    assertEquals("content://provider/playing.flac", state.currentTrackUri)
    assertEquals(1_250L, state.positionMs)
    assertEquals(180_000L, state.durationMs)
    assertEquals(1_250f / 180_000f, state.progressFraction, 0.000_001f)
    assertTrue(state.shuffled)
    assertEquals(AndroidRepeatMode.ALL, state.repeat)
}

@Test
fun pausedPlaybackOffersPlayOnTheSurface() {
    val state = AndroidPlaybackSnapshot(
        state = AndroidPlaybackState.PAUSED,
        currentIndex = 0u,
        currentTrackId = 41,
        currentTrackUri = "content://provider/playing.flac",
        positionMs = 0,
        durationMs = 0,
        automaticAdvanceCount = 0u,
        shuffled = false,
        repeat = AndroidRepeatMode.OFF,
        error = null,
    ).toUiState()

    assertEquals("Play", state.playPauseLabel)
}

@Test
fun bufferingPlaybackOffersOnePauseSymbolAndLabel() {
    val state = AndroidPlaybackSnapshot(
        state = AndroidPlaybackState.BUFFERING,
        currentIndex = 0u,
        currentTrackId = 41,
        currentTrackUri = "content://provider/playing.flac",
        positionMs = 0,
        durationMs = 0,
        automaticAdvanceCount = 0u,
        shuffled = false,
        repeat = AndroidRepeatMode.OFF,
        error = null,
    ).toUiState()

    assertEquals("pause", state.playPauseSymbol)
    assertEquals("Pause", state.playPauseLabel)
}

@Test
fun applicationLooperDispatchRunsInlineOnItsOwningThread() {
    var postCount = 0
    val dispatch = ApplicationLooperDispatch(
        isApplicationThread = { true },
        post = {
            postCount += 1
            true
        },
    )

    val executingThread = dispatch.call { Thread.currentThread().name }

    assertEquals(Thread.currentThread().name, executingThread)
    assertEquals(0, postCount)
}

@Test
fun applicationLooperDispatchPostsAndWaitsFromAnotherThread() {
    val worker = AtomicReference<Thread>()
    val dispatch = ApplicationLooperDispatch(
        isApplicationThread = { false },
        post = { command ->
            worker.set(Thread(command, "media3-application-looper").apply(Thread::start))
            true
        },
    )

    val executingThread = dispatch.call { Thread.currentThread().name }

    worker.get().join()
    assertEquals("media3-application-looper", executingThread)
}

@Test
fun unknownTotalUsesIndeterminateProgress() {
    val scanning = LibraryScreenState.Scanning(processed = 1u, total = null)

    assertEquals(ScanProgressPresentation.Indeterminate, scanning.progressPresentation())
}

@Test
fun knownTotalUsesHonestProgressFraction() {
    val scanning = LibraryScreenState.Scanning(processed = 1u, total = 4u)

    assertEquals(ScanProgressPresentation.Determinate(0.25f), scanning.progressPresentation())
}

@Test
fun rememberedReadableTreeLoadsOnlyTheRememberedDestinationWithoutScanning() {
    val port = RecordingLibrarySessionPort(
        rememberedTreeUri = "content://provider/tree/Music",
        readable = true,
        tracks = listOf(testTrack()),
    )

    val state = LibrarySession(port).restore(BrowseTab.ARTISTS)

    assertEquals(
        LibraryScreenState.Browse(
            titles = LibraryWindow(total = 1, rows = emptyList(), hasMore = false),
            artists = completeTestWindow(emptyList()),
            folderUri = "content://provider/tree/Music",
            loadedTabs = setOf(BrowseTab.ARTISTS),
        ),
        state,
    )
    assertEquals(listOf("content://provider/tree/Music"), port.configuredUris)
    assertEquals(
        listOf(
            "readable:content://provider/tree/Music",
            "configure:content://provider/tree/Music",
            "search::0:1",
            "artists:0:200",
            "search-albums::0:1",
        ),
        port.operations,
    )
    assertEquals(1, port.listCalls)
    assertEquals(0, port.scanCalls)
}

@Test
fun rememberedArtistsDestinationRetainsTheTrueTitleTotalForSettings() {
    val port = RecordingLibrarySessionPort(
        rememberedTreeUri = "content://provider/tree/Music",
        readable = true,
        tracks = listOf(testTrack()),
    )

    val state = LibrarySession(port).restore(BrowseTab.ARTISTS)
        as LibraryScreenState.Browse

    assertEquals(1, state.titles.total)
    assertTrue(state.titles.rows.isEmpty())
}

@Test
fun rememberedUnreadableTreeDoesNotTouchCatalog() {
    val port = RecordingLibrarySessionPort(
        rememberedTreeUri = "content://provider/tree/Music",
        readable = false,
        tracks = listOf(testTrack()),
    )

    val state = LibrarySession(port).restore()

    assertEquals(LibraryScreenState.TreeUnreadable, state)
    assertTrue(port.configuredUris.isEmpty())
    assertEquals(0, port.listCalls)
    assertEquals(0, port.scanCalls)
}

@Test
fun choosingTreePersistsGrantAndPreferenceBeforeScanning() {
    val port = RecordingLibrarySessionPort(
        rememberedTreeUri = null,
        readable = true,
        tracks = listOf(testTrack()),
    )
    val reports = mutableListOf<LibraryScreenState.Scanning>()

    val state = LibrarySession(port).chooseTree("content://provider/tree/Music", reports::add)

    assertEquals(
        LibraryScreenState.Browse(
            titles = completeTestWindow(listOf(testTrack())),
            artists = completeTestWindow(emptyList()),
            folderUri = "content://provider/tree/Music",
        ),
        state,
    )
    assertEquals(
        listOf(
            "persist:content://provider/tree/Music",
            "remember:content://provider/tree/Music",
            "readable:content://provider/tree/Music",
            "configure:content://provider/tree/Music",
            "scan",
            "search::0:200",
            "artists:0:200",
            "search-albums::0:1",
        ),
        port.operations,
    )
    assertEquals(LibraryScreenState.Scanning(), reports.first())
}

@Test
fun aFinishedScanStartsThePrefetch() {
    val port = RecordingLibrarySessionPort(
        rememberedTreeUri = null,
        readable = true,
        tracks = emptyList(),
    )
    var starts = 0
    val session = LibrarySession(
        port = port,
        startPortraitPrefetch = { starts += 1 },
    )

    session.chooseTree("content://provider/tree/Music") { }

    assertEquals(1, starts)
}

@Test
fun thePrefetchNeverRunsInsideTheScan() {
    val port = RecordingLibrarySessionPort(
        rememberedTreeUri = null,
        readable = true,
        tracks = emptyList(),
    )
    val session = LibrarySession(
        port = port,
        startPortraitPrefetch = { port.operations += "prefetch" },
    )

    session.chooseTree("content://provider/tree/Music") { }

    assertTrue(port.operations.indexOf("prefetch") > port.operations.indexOf("scan"))
}

@Test
fun restoringAnExistingLibraryStartsThePrefetch() {
    val port = RecordingLibrarySessionPort(
        rememberedTreeUri = "content://provider/tree/Music",
        readable = true,
        tracks = emptyList(),
    )
    var starts = 0
    val session = LibrarySession(
        port = port,
        startPortraitPrefetch = { starts += 1 },
    )

    session.restore()

    assertEquals(1, starts)
}

@Test
fun rescanUsesRememberedTreeWithoutChoosingAgain() {
    val port = RecordingLibrarySessionPort(
        rememberedTreeUri = "content://provider/tree/Music",
        readable = true,
        tracks = listOf(testTrack()),
    )
    val reports = mutableListOf<LibraryScreenState.Scanning>()

    val state = LibrarySession(port).rescan(reports::add)

    assertEquals(
        LibraryScreenState.Browse(
            titles = completeTestWindow(listOf(testTrack())),
            artists = completeTestWindow(emptyList()),
            folderUri = "content://provider/tree/Music",
        ),
        state,
    )
    assertEquals(
        listOf(
            "readable:content://provider/tree/Music",
            "configure:content://provider/tree/Music",
            "scan",
            "search::0:200",
            "artists:0:200",
            "search-albums::0:1",
        ),
        port.operations,
    )
    assertEquals(LibraryScreenState.Scanning(), reports.first())
}

@Test
fun automaticScanIsSilentAndRefreshesAfterFiveMinutes() {
    val port = RecordingLibrarySessionPort(
        rememberedTreeUri = "content://provider/tree/Music",
        readable = true,
        tracks = listOf(testTrack()),
        lastScanCompletedAtMs = 1_000L,
    )
    val session = LibrarySession(port, nowMillis = { 301_000L })

    val state = session.autoScan()

    assertEquals(1, port.scanCalls)
    assertEquals(301_000L, port.lastScanCompletedAtMs())
    assertEquals(1, (state as LibraryScreenState.Browse).titles.total)
}

@Test
fun automaticScanDoesNothingInsideTheFiveMinuteWindow() {
    val port = RecordingLibrarySessionPort(
        rememberedTreeUri = "content://provider/tree/Music",
        readable = true,
        tracks = listOf(testTrack()),
        lastScanCompletedAtMs = 2_000L,
    )
    val session = LibrarySession(port, nowMillis = { 301_999L })

    assertEquals(null, session.autoScan())
    assertEquals(0, port.scanCalls)
}

@Test
fun automaticScanDoesNothingWhenTheClockMovesBehindTheSavedTime() {
    val port = RecordingLibrarySessionPort(
        rememberedTreeUri = "content://provider/tree/Music",
        readable = true,
        tracks = emptyList(),
        lastScanCompletedAtMs = 500_000L,
    )

    assertEquals(null, LibrarySession(port, nowMillis = { 400_000L }).autoScan())
    assertEquals(0, port.scanCalls)
}

@Test
fun replacementSessionsCannotOverlapAnAutomaticScan() {
    val firstScanEntered = CountDownLatch(1)
    val releaseFirstScan = CountDownLatch(1)
    val inFlight = AtomicInteger()
    val maximumInFlight = AtomicInteger()
    val port = RecordingLibrarySessionPort(
        rememberedTreeUri = "content://provider/tree/Music",
        readable = true,
        tracks = emptyList(),
        scanAction = {
            val current = inFlight.incrementAndGet()
            maximumInFlight.accumulateAndGet(current, ::maxOf)
            firstScanEntered.countDown()
            releaseFirstScan.await(2, TimeUnit.SECONDS)
            inFlight.decrementAndGet()
        },
    )
    val retainedState = MobileSurfaceViewModel()
    val firstSession = LibrarySession(
        port,
        nowMillis = { 301_000L },
        scanMonitor = retainedState.libraryScanMonitor,
    )
    val replacementSession = LibrarySession(
        port,
        nowMillis = { 301_001L },
        scanMonitor = retainedState.libraryScanMonitor,
    )
    val firstWorker = Thread { firstSession.autoScan() }
    val replacementWorker = Thread { replacementSession.autoScan() }

    firstWorker.start()
    assertTrue(firstScanEntered.await(1, TimeUnit.SECONDS))
    replacementWorker.start()
    val overlapDeadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(1)
    while (maximumInFlight.get() < 2 && System.nanoTime() < overlapDeadline) {
        Thread.yield()
    }
    releaseFirstScan.countDown()
    firstWorker.join(2_000)
    replacementWorker.join(2_000)

    assertEquals(1, maximumInFlight.get())
    assertEquals(1, port.scanCalls)
}

@Test
fun failedAutomaticScanStartsTheSameCooldownAsACompletedScan() {
    val port = RecordingLibrarySessionPort(
        rememberedTreeUri = "content://provider/tree/Music",
        readable = true,
        tracks = emptyList(),
        scanAction = { throw IllegalStateException("scan failed") },
    )
    val session = LibrarySession(port, nowMillis = { 301_000L })

    assertThrows(IllegalStateException::class.java) { session.autoScan() }

    assertEquals(301_000L, port.lastScanCompletedAtMs())
    assertEquals(null, LibrarySession(port, nowMillis = { 301_001L }).autoScan())
    assertEquals(1, port.scanCalls)
}

@Test
fun manualScanPersistsItsCompletionTime() {
    val port = RecordingLibrarySessionPort(
        rememberedTreeUri = "content://provider/tree/Music",
        readable = true,
        tracks = emptyList(),
    )

    LibrarySession(port, nowMillis = { 42_000L }).rescan { }

    assertEquals(42_000L, port.lastScanCompletedAtMs())
}

@Test
fun artistAlbumWindowsDelegateTheLiteralArtistAndWindow() {
    val port = RecordingLibrarySessionPort(
        rememberedTreeUri = null,
        readable = true,
        tracks = emptyList(),
    )
    val artist = LibraryArtist(" Slowdive ", 9, 3, "content://slowdive")
    val window = LibraryWindowRange(offset = 200, limit = 75)

    LibrarySession(port).listArtistAlbums(artist, window)

    assertEquals(listOf("artist-albums: Slowdive :200:75"), port.operations)
}

@Test
fun artistUntaggedWindowsDelegateTheLiteralArtistAndWindow() {
    val port = RecordingLibrarySessionPort(
        rememberedTreeUri = null,
        readable = true,
        tracks = emptyList(),
    )
    val artist = LibraryArtist(" Low ", 7, 2, "content://low")
    val window = LibraryWindowRange(offset = 125, limit = 33)

    LibrarySession(port).listArtistUntaggedTracks(artist, window)

    assertEquals(listOf("artist-untagged: Low :125:33"), port.operations)
}

@Test
fun portraitLookupsAreNotMemoisedBySession() {
    val port = RecordingLibrarySessionPort(
        rememberedTreeUri = null,
        readable = true,
        tracks = emptyList(),
    )
    val session = LibrarySession(port)

    session.artistPortraitCached("Low", AndroidArtworkSize.LIST)
    session.artistPortraitCached("Low", AndroidArtworkSize.LIST)

    assertEquals(
        listOf("portrait-cached:Low:LIST", "portrait-cached:Low:LIST"),
        port.operations,
    )
}
}

private fun testTrack() = LibraryTrack(
    id = 1,
    uri = "content://provider/document/song.flac",
    title = "Song",
    artist = "Artist",
    album = "Album",
    durationMs = 1_000,
    playCount = 27,
    rating = 4,
)

private fun <T> completeTestWindow(rows: List<T>) = LibraryWindow(
    total = rows.size.toLong(),
    rows = rows,
    hasMore = false,
)

private class RecordingLibrarySessionPort(
    rememberedTreeUri: String?,
    private val readable: Boolean,
    private val tracks: List<LibraryTrack>,
    private val artistTracks: List<LibraryTrack> = emptyList(),
    lastScanCompletedAtMs: Long = 0L,
    private val scanAction: () -> Unit = {},
) : LibrarySessionPort {
    private var remembered = rememberedTreeUri
    private var lastScanCompleted = lastScanCompletedAtMs
    val configuredUris = mutableListOf<String>()
    val operations = mutableListOf<String>()
    var listCalls = 0
    private val scanCallCount = AtomicInteger()
    val scanCalls: Int
        get() = scanCallCount.get()

    override fun rememberedTreeUri(): String? = remembered

    override fun rememberTreeUri(treeUri: String) {
        operations += "remember:$treeUri"
        remembered = treeUri
    }

    override fun persistTreePermission(treeUri: String) {
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
        scanCallCount.incrementAndGet()
        scanAction()
    }

    override fun lastScanCompletedAtMs(): Long = lastScanCompleted

    override fun rememberScanCompletedAtMs(completedAtMs: Long) {
        lastScanCompleted = completedAtMs
    }

    override fun searchTracks(
        text: String,
        window: LibraryWindowRange,
    ): LibraryWindow<LibraryTrack> {
        operations += "search:$text:${window.offset}:${window.limit}"
        listCalls += 1
        return completeTestWindow(tracks)
    }

    override fun searchAlbums(
        text: String,
        window: LibraryWindowRange,
    ): LibraryWindow<LibraryAlbum> {
        operations += "search-albums:$text:${window.offset}:${window.limit}"
        return completeTestWindow(emptyList())
    }

    override fun listArtists(window: LibraryWindowRange): LibraryWindow<LibraryArtist> {
        operations += "artists:${window.offset}:${window.limit}"
        return completeTestWindow(emptyList())
    }

    override fun searchArtists(
        text: String,
        window: LibraryWindowRange,
    ): LibraryWindow<LibraryArtist> {
        operations += "search-artists:$text:${window.offset}:${window.limit}"
        return completeTestWindow(emptyList())
    }

    override fun listArtistAlbums(
        artist: String,
        window: LibraryWindowRange,
    ): LibraryWindow<LibraryAlbum> {
        operations += "artist-albums:$artist:${window.offset}:${window.limit}"
        return completeTestWindow(emptyList())
    }

    override fun listArtistUntaggedTracks(
        artist: String,
        window: LibraryWindowRange,
    ): LibraryWindow<LibraryTrack> {
        operations += "artist-untagged:$artist:${window.offset}:${window.limit}"
        return completeTestWindow(emptyList())
    }

    override fun listArtistTracks(
        artist: String,
        window: LibraryWindowRange,
    ): LibraryWindow<LibraryTrack> {
        operations += "artist:$artist:${window.offset}:${window.limit}"
        return completeTestWindow(artistTracks)
    }

    override fun listAlbumTracks(
        album: String,
        albumArtist: String,
        window: LibraryWindowRange,
    ): LibraryWindow<LibraryTrack> = completeTestWindow(emptyList())

    override fun albumTrackIds(album: String, albumArtist: String): List<Long> = emptyList()

    override fun trackById(trackId: Long): LibraryTrack? = tracks.firstOrNull { it.id == trackId }

    override fun artworkFor(trackUri: String, size: AndroidArtworkSize): String? = null

    override fun artistPortraitCached(name: String, size: AndroidArtworkSize): String? {
        operations += "portrait-cached:$name:${size.name}"
        return null
    }

    override fun artistPortraitFetched(name: String, size: AndroidArtworkSize): String? = null

    override fun artistsMissingPortraits(limit: UInt): List<String> = emptyList()

    override fun setFavourite(trackId: Long, favourite: Boolean) = Unit
}
