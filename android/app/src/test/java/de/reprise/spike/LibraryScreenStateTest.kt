package de.reprise.spike

import androidx.media3.common.Player
import java.lang.reflect.Proxy
import java.util.concurrent.atomic.AtomicReference
import uniffi.reprise_android_ffi.AndroidPlaybackSnapshot
import uniffi.reprise_android_ffi.AndroidPlaybackState

fun main() {
    unknownTotalUsesIndeterminateProgress()
    knownTotalUsesHonestProgressFraction()
    rememberedReadableTreeListsCatalogWithoutScanning()
    rememberedUnreadableTreeDoesNotTouchCatalog()
    choosingTreePersistsGrantAndPreferenceBeforeScanning()
    rescanUsesRememberedTreeWithoutChoosingAgain()
    applicationLooperDispatchRunsInlineOnItsOwningThread()
    applicationLooperDispatchPostsAndWaitsFromAnotherThread()
    positionReadoutUsesTheDurationDeliveredByTheBridge()
    pausedPlaybackOffersPlayOnTheSurface()
    mediaSessionTransportReturnsToCore()
}

private fun mediaSessionTransportReturnsToCore() {
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

    check(calls == listOf("toggle", "toggle", "next", "previous"))
}

private fun primitiveDefault(type: Class<*>): Any? = when (type) {
    Boolean::class.javaPrimitiveType -> false
    Int::class.javaPrimitiveType -> 0
    Long::class.javaPrimitiveType -> 0L
    Float::class.javaPrimitiveType -> 0f
    Double::class.javaPrimitiveType -> 0.0
    else -> null
}

private fun positionReadoutUsesTheDurationDeliveredByTheBridge() {
    val state = AndroidPlaybackSnapshot(
        state = AndroidPlaybackState.PLAYING,
        currentIndex = 2u,
        positionMs = 1_250,
        durationMs = 180_000,
        error = null,
    ).toUiState()

    check(state.positionReadout == "0:01 / 3:00")
    check(state.currentIndex == 2)
}

private fun pausedPlaybackOffersPlayOnTheSurface() {
    val state = AndroidPlaybackSnapshot(
        state = AndroidPlaybackState.PAUSED,
        currentIndex = 0u,
        positionMs = 0,
        durationMs = 0,
        error = null,
    ).toUiState()

    check(state.playPauseLabel == "Play")
}

private fun applicationLooperDispatchRunsInlineOnItsOwningThread() {
    var postCount = 0
    val dispatch = ApplicationLooperDispatch(
        isApplicationThread = { true },
        post = {
            postCount += 1
            true
        },
    )

    val executingThread = dispatch.call { Thread.currentThread().name }

    check(executingThread == Thread.currentThread().name)
    check(postCount == 0)
}

private fun applicationLooperDispatchPostsAndWaitsFromAnotherThread() {
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
    check(executingThread == "media3-application-looper")
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

    check(
        state == LibraryScreenState.Browse(
            titles = listOf(testTrack()),
            albums = emptyList(),
            artists = emptyList(),
        ),
    )
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

    check(
        state == LibraryScreenState.Browse(
            titles = listOf(testTrack()),
            albums = emptyList(),
            artists = emptyList(),
        ),
    )
    check(
        port.operations == listOf(
            "persist:content://provider/tree/Music",
            "remember:content://provider/tree/Music",
            "readable:content://provider/tree/Music",
            "configure:content://provider/tree/Music",
            "scan",
            "search:",
            "albums",
            "artists",
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

    check(
        state == LibraryScreenState.Browse(
            titles = listOf(testTrack()),
            albums = emptyList(),
            artists = emptyList(),
        ),
    )
    check(
        port.operations == listOf(
            "readable:content://provider/tree/Music",
            "configure:content://provider/tree/Music",
            "scan",
            "search:",
            "albums",
            "artists",
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

    override fun searchTracks(text: String): List<LibraryTrack> {
        operations += "search:$text"
        listCalls += 1
        return tracks
    }

    override fun listAlbums(): List<LibraryAlbum> {
        operations += "albums"
        return emptyList()
    }

    override fun listArtists(): List<LibraryArtist> {
        operations += "artists"
        return emptyList()
    }

    override fun listAlbumTracks(album: String, albumArtist: String): List<LibraryTrack> =
        emptyList()
}
