package de.reprise.spike

import androidx.media3.common.Player
import java.lang.reflect.Proxy
import java.util.concurrent.atomic.AtomicReference
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.reprise_android_ffi.AndroidPlaybackSnapshot
import uniffi.reprise_android_ffi.AndroidPlaybackState

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
fun positionReadoutUsesTheDurationDeliveredByTheBridge() {
    val state = AndroidPlaybackSnapshot(
        state = AndroidPlaybackState.PLAYING,
        currentIndex = 2u,
        positionMs = 1_250,
        durationMs = 180_000,
        error = null,
    ).toUiState()

    assertEquals("0:01 / 3:00", state.positionReadout)
    assertEquals(2, state.currentIndex)
}

@Test
fun pausedPlaybackOffersPlayOnTheSurface() {
    val state = AndroidPlaybackSnapshot(
        state = AndroidPlaybackState.PAUSED,
        currentIndex = 0u,
        positionMs = 0,
        durationMs = 0,
        error = null,
    ).toUiState()

    assertEquals("Play", state.playPauseLabel)
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
fun rememberedReadableTreeListsCatalogWithoutScanning() {
    val port = RecordingLibrarySessionPort(
        rememberedTreeUri = "content://provider/tree/Music",
        readable = true,
        tracks = listOf(testTrack()),
    )

    val state = LibrarySession(port).restore()

    assertEquals(
        LibraryScreenState.Browse(
            titles = completeTestWindow(listOf(testTrack())),
            albums = completeTestWindow(emptyList()),
            artists = completeTestWindow(emptyList()),
        ),
        state,
    )
    assertEquals(listOf("content://provider/tree/Music"), port.configuredUris)
    assertEquals(1, port.listCalls)
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
            albums = completeTestWindow(emptyList()),
            artists = completeTestWindow(emptyList()),
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
            "albums:0:200",
            "artists:0:200",
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
            albums = completeTestWindow(emptyList()),
            artists = completeTestWindow(emptyList()),
        ),
        state,
    )
    assertEquals(
        listOf(
            "readable:content://provider/tree/Music",
            "configure:content://provider/tree/Music",
            "scan",
            "search::0:200",
            "albums:0:200",
            "artists:0:200",
        ),
        port.operations,
    )
    assertEquals(LibraryScreenState.Scanning(), reports.first())
}
}

private fun testTrack() = LibraryTrack(
    uri = "content://provider/document/song.flac",
    title = "Song",
    artist = "Artist",
    album = "Album",
    durationMs = 1_000,
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

    override fun listArtists(window: LibraryWindowRange): LibraryWindow<LibraryArtist> {
        operations += "artists:${window.offset}:${window.limit}"
        return completeTestWindow(emptyList())
    }

    override fun listAlbumTracks(
        album: String,
        albumArtist: String,
        window: LibraryWindowRange,
    ): LibraryWindow<LibraryTrack> = completeTestWindow(emptyList())
}
