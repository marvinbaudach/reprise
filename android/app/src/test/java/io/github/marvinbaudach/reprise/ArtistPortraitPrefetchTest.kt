package io.github.marvinbaudach.reprise

import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.reprise_android_ffi.AndroidArtworkSize

private const val PREFETCH_WAIT_SECONDS = 2L

class ArtistPortraitPrefetchTest {
    @Test
    fun thePrefetchFetchesEveryNameTheBridgeReports() {
        val fetched = CopyOnWriteArrayList<String>()
        val allFetched = CountDownLatch(3)
        val port = PrefetchLibrarySessionPort(
            missingPortraits = queuedResponses(
                listOf("Low", "Miles"),
                listOf("Nina"),
                emptyList(),
            ),
            fetchPortrait = { name ->
                fetched += name
                allFetched.countDown()
                null
            },
        )
        val prefetch = ArtistPortraitPrefetch(port)

        try {
            prefetch.start()

            assertTrue(allFetched.await(PREFETCH_WAIT_SECONDS, TimeUnit.SECONDS))
            assertEquals(listOf("Low", "Miles", "Nina"), fetched)
        } finally {
            prefetch.shutdown()
        }
    }

    @Test
    fun thePrefetchFetchesEachArtistOnlyOnce() {
        val fetched = CopyOnWriteArrayList<String>()
        val queriedTwice = CountDownLatch(2)
        val port = PrefetchLibrarySessionPort(
            missingPortraits = {
                queriedTwice.countDown()
                listOf("Low", "Low")
            },
            fetchPortrait = { name -> fetched += name; null },
        )
        val prefetch = ArtistPortraitPrefetch(port)

        try {
            prefetch.start()

            assertTrue(queriedTwice.await(PREFETCH_WAIT_SECONDS, TimeUnit.SECONDS))
            assertEquals(listOf("Low"), fetched)
        } finally {
            prefetch.shutdown()
        }
    }

    @Test
    fun anEmptyListEndsThePrefetchWithoutFetching() {
        val queried = CountDownLatch(1)
        val fetched = CopyOnWriteArrayList<String>()
        val port = PrefetchLibrarySessionPort(
            missingPortraits = {
                queried.countDown()
                emptyList()
            },
            fetchPortrait = { name -> fetched += name; null },
        )
        val prefetch = ArtistPortraitPrefetch(port)

        try {
            prefetch.start()

            assertTrue(queried.await(PREFETCH_WAIT_SECONDS, TimeUnit.SECONDS))
            assertTrue(fetched.isEmpty())
        } finally {
            prefetch.shutdown()
        }
    }

    @Test
    fun shutdownStopsThePrefetchBeforeTheNextFetch() {
        val firstFetchStarted = CountDownLatch(1)
        val releaseFirstFetch = CountDownLatch(1)
        val secondFetchStarted = CountDownLatch(1)
        val port = PrefetchLibrarySessionPort(
            missingPortraits = queuedResponses(listOf("Low", "Miles"), emptyList()),
            fetchPortrait = { name ->
                if (name == "Low") {
                    firstFetchStarted.countDown()
                    releaseFirstFetch.awaitUninterruptibly()
                } else {
                    secondFetchStarted.countDown()
                }
                null
            },
        )
        val prefetch = ArtistPortraitPrefetch(port)

        prefetch.start()
        assertTrue(firstFetchStarted.await(PREFETCH_WAIT_SECONDS, TimeUnit.SECONDS))
        prefetch.shutdown()
        releaseFirstFetch.countDown()

        assertFalse(secondFetchStarted.await(200, TimeUnit.MILLISECONDS))
    }

    @Test
    fun aFailingFetchDoesNotStopTheRest() {
        val attempted = CopyOnWriteArrayList<String>()
        val allAttempted = CountDownLatch(3)
        val port = PrefetchLibrarySessionPort(
            missingPortraits = queuedResponses(
                listOf("Low", "Broken", "Nina"),
                emptyList(),
            ),
            fetchPortrait = { name ->
                attempted += name
                allAttempted.countDown()
                if (name == "Broken") error("fetch failed")
                null
            },
        )
        val prefetch = ArtistPortraitPrefetch(port)

        try {
            prefetch.start()

            assertTrue(allAttempted.await(PREFETCH_WAIT_SECONDS, TimeUnit.SECONDS))
            assertEquals(listOf("Low", "Broken", "Nina"), attempted)
        } finally {
            prefetch.shutdown()
        }
    }

    @Test
    fun aTransientlyFailingWholeBatchDoesNotHideLaterArtists() {
        val transientNames = List(32) { index -> "Transient $index" }
        val laterNames = listOf("Later One", "Later Two")
        val allNames = transientNames + laterNames
        val fetchedLater = CountDownLatch(laterNames.size)
        val port = PrefetchLibrarySessionPort(
            missingPortraits = { limit -> allNames.take(limit.toInt()) },
            fetchPortrait = { name ->
                if (name in transientNames) error("transient fetch failure")
                fetchedLater.countDown()
                null
            },
        )
        val prefetch = ArtistPortraitPrefetch(port)

        try {
            prefetch.start()

            assertTrue(fetchedLater.await(PREFETCH_WAIT_SECONDS, TimeUnit.SECONDS))
        } finally {
            prefetch.shutdown()
        }
    }
}

private fun queuedResponses(vararg responses: List<String>): (UInt) -> List<String> {
    val remaining = ArrayDeque(responses.toList())
    return { limit ->
        assertEquals(32u, limit)
        if (remaining.isEmpty()) emptyList() else remaining.removeFirst()
    }
}

private fun CountDownLatch.awaitUninterruptibly() {
    var interrupted = false
    while (true) {
        try {
            await()
            break
        } catch (_: InterruptedException) {
            interrupted = true
        }
    }
    if (interrupted) {
        Thread.currentThread().interrupt()
    }
}

private class PrefetchLibrarySessionPort(
    private val missingPortraits: (UInt) -> List<String>,
    private val fetchPortrait: (String) -> String?,
) : LibrarySessionPort {
    override fun rememberedTreeUri(): String? = null

    override fun rememberTreeUri(treeUri: String) = Unit

    override fun persistTreePermission(treeUri: String) = Unit

    override fun isTreeReadable(treeUri: String): Boolean = true

    override fun configureTree(treeUri: String) = Unit

    override fun scan(report: (LibraryScreenState.Scanning) -> Unit) = Unit

    override fun searchTracks(
        text: String,
        window: LibraryWindowRange,
    ): LibraryWindow<LibraryTrack> = LibraryWindow.empty()

    override fun searchAlbums(
        text: String,
        window: LibraryWindowRange,
    ): LibraryWindow<LibraryAlbum> = LibraryWindow.empty()

    override fun listArtists(window: LibraryWindowRange): LibraryWindow<LibraryArtist> =
        LibraryWindow.empty()

    override fun searchArtists(
        text: String,
        window: LibraryWindowRange,
    ): LibraryWindow<LibraryArtist> = LibraryWindow.empty()

    override fun listArtistAlbums(
        artist: String,
        window: LibraryWindowRange,
    ): LibraryWindow<LibraryAlbum> = LibraryWindow.empty()

    override fun listArtistUntaggedTracks(
        artist: String,
        window: LibraryWindowRange,
    ): LibraryWindow<LibraryTrack> = LibraryWindow.empty()

    override fun listArtistTracks(
        artist: String,
        window: LibraryWindowRange,
    ): LibraryWindow<LibraryTrack> = LibraryWindow.empty()

    override fun listAlbumTracks(
        album: String,
        albumArtist: String,
        window: LibraryWindowRange,
    ): LibraryWindow<LibraryTrack> = LibraryWindow.empty()

    override fun albumTrackIds(album: String, albumArtist: String): List<Long> = emptyList()

    override fun trackById(trackId: Long): LibraryTrack? = null

    override fun artworkFor(trackUri: String, size: AndroidArtworkSize): String? = null

    override fun artistPortraitCached(name: String, size: AndroidArtworkSize): String? = null

    override fun artistPortraitFetched(name: String, size: AndroidArtworkSize): String? =
        fetchPortrait(name)

    override fun artistsMissingPortraits(limit: UInt): List<String> = missingPortraits(limit)

    override fun setFavourite(trackId: Long, favourite: Boolean) = Unit
}
