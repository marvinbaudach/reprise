package de.reprise.spike

import androidx.media3.common.Player
import java.lang.reflect.Proxy
import java.util.concurrent.atomic.AtomicReference
import org.junit.Assert.assertEquals
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

        override fun previous() {
            calls += "previous"
        }
    })

    controlled.play()
    playWhenReady = true
    controlled.pause()
    controlled.seekToNext()
    controlled.seekToPreviousMediaItem()

    assertEquals(listOf("toggle", "toggle", "next", "previous"), calls)
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
            titles = LibraryWindow.empty(),
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
            "artists:0:200",
            "search-albums::0:1",
        ),
        port.operations,
    )
    assertEquals(0, port.listCalls)
    assertEquals(0, port.scanCalls)
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
    private val favouriteTracks: List<LibraryTrack> = emptyList(),
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
        scanCalls += 1
    }

    override fun searchTracks(
        text: String,
        window: LibraryWindowRange,
    ): LibraryWindow<LibraryTrack> {
        operations += "search:$text:${window.offset}:${window.limit}"
        listCalls += 1
        return completeTestWindow(tracks)
    }

    override fun listAlbums(window: LibraryWindowRange): LibraryWindow<LibraryAlbum> {
        operations += "albums:${window.offset}:${window.limit}"
        return completeTestWindow(emptyList())
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

    override fun listFavourites(window: LibraryWindowRange): LibraryWindow<LibraryTrack> {
        operations += "favourites:${window.offset}:${window.limit}"
        return completeTestWindow(favouriteTracks)
    }

    override fun searchFavourites(
        text: String,
        window: LibraryWindowRange,
    ): LibraryWindow<LibraryTrack> {
        operations += "search-favourites:$text:${window.offset}:${window.limit}"
        return completeTestWindow(favouriteTracks)
    }

    override fun listAlbumTracks(
        album: String,
        albumArtist: String,
        window: LibraryWindowRange,
    ): LibraryWindow<LibraryTrack> = completeTestWindow(emptyList())

    override fun albumTrackIds(album: String, albumArtist: String): List<Long> = emptyList()

    override fun trackById(trackId: Long): LibraryTrack? = tracks.firstOrNull { it.id == trackId }

    override fun artworkFor(trackUri: String, size: AndroidArtworkSize): String? = null

    override fun setFavourite(trackId: Long, favourite: Boolean) = Unit
}
