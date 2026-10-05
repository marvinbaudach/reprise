package io.github.marvinbaudach.reprise

import android.os.Looper
import androidx.media3.session.LibraryResult
import androidx.media3.session.MediaBrowser
import com.google.common.util.concurrent.ListenableFuture
import io.github.marvinbaudach.reprise.library.BrowseAlbum
import io.github.marvinbaudach.reprise.library.BrowseArtist
import io.github.marvinbaudach.reprise.library.BrowsePage
import io.github.marvinbaudach.reprise.library.BrowsePlaylist
import io.github.marvinbaudach.reprise.library.BrowseTrack
import io.github.marvinbaudach.reprise.library.MediaBrowseLibrary
import io.github.marvinbaudach.reprise.library.TrackMetadata
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.android.controller.ServiceController
import org.robolectric.annotation.Config
import uniffi.reprise_android_ffi.AndroidPlaybackSession

private const val AWAIT_LIMIT_MS = 10_000L

/**
 * The service's own part of the browse tree: that it builds a library session,
 * words its folders from resources, and turns a tapped song into a Core play
 * request. The tree and the callback are covered one level down.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class PlaybackServiceBrowseTest {
    private val controllers = mutableListOf<ServiceController<BrowsingPlaybackService>>()

    @After
    fun releaseTheServices() {
        controllers.forEach(ServiceController<BrowsingPlaybackService>::destroy)
    }

    private fun service(): BrowsingPlaybackService =
        Robolectric.buildService(BrowsingPlaybackService::class.java)
            .create()
            .also(controllers::add)
            .get()

    private fun <T> await(future: ListenableFuture<T>): T {
        val deadline = System.currentTimeMillis() + AWAIT_LIMIT_MS
        while (!future.isDone) {
            shadowOf(Looper.getMainLooper()).idle()
            check(System.currentTimeMillis() < deadline) { "timed out waiting for Media3" }
            Thread.sleep(5)
        }
        shadowOf(Looper.getMainLooper()).idle()
        return future.get()
    }

    private fun awaitUntil(condition: () -> Boolean) {
        val deadline = System.currentTimeMillis() + AWAIT_LIMIT_MS
        while (!condition()) {
            shadowOf(Looper.getMainLooper()).idle()
            check(System.currentTimeMillis() < deadline) { "timed out waiting for the play request" }
            Thread.sleep(5)
        }
    }

    private fun browserFor(service: ReprisePlaybackService): MediaBrowser =
        await(
            MediaBrowser.Builder(
                RuntimeEnvironment.getApplication(),
                checkNotNull(service.librarySession).token,
            ).buildAsync(),
        )

    @Test
    fun theServiceBuildsALibrarySessionForBrowsers() {
        assertNotNull(service().librarySession)
    }

    @Test
    fun theFoldersAreWordedFromResources() {
        val tree = service().browseTree()

        assertEquals(
            listOf("Recently played", "Playlists", "Albums", "Artists"),
            tree.children("root", 0, 10)!!.map { it.mediaMetadata.title.toString() },
        )
        assertEquals("Reprise", tree.root().mediaMetadata.title.toString())
    }

    @Test
    fun aBrowserWalksTheTreeThroughTheServicesSession() {
        val service = service()
        val browser = browserFor(service)

        val songs = await(browser.getChildren("recent", 0, 10, null))

        assertEquals(LibraryResult.RESULT_SUCCESS, songs.resultCode)
        assertEquals(listOf("Track 3", "Track 1"), songs.value!!.map { it.mediaMetadata.title.toString() })
        browser.release()
    }

    @Test
    fun aSongTappedByABrowserIsPlayedThroughTheCoreAsItsContainer() {
        val service = service()
        val browser = browserFor(service)
        val songs = await(browser.getChildren("recent", 0, 10, null))

        browser.setMediaItem(songs.value!![1])
        browser.prepare()
        browser.play()
        awaitUntil { service.playRequests.isNotEmpty() }

        assertEquals(listOf(listOf(3L, 1L) to 1), service.playRequests)
        browser.release()
    }

    @Test
    fun aFailureInTheCoreDoesNotCrashTheService() {
        val service = service()
        service.failPlayback = true
        val browser = browserFor(service)
        val songs = await(browser.getChildren("recent", 0, 10, null))

        browser.setMediaItem(songs.value!![0])
        awaitUntil { service.playRequests.isNotEmpty() }

        assertEquals(1, service.playRequests.size)
        browser.release()
    }
}

/** The real service with the native library replaced by plain fixtures. */
internal class BrowsingPlaybackService : ReprisePlaybackService() {
    val playRequests = mutableListOf<Pair<List<Long>, Int>>()
    var failPlayback = false

    override fun openCoreSession(
        port: Media3PlaybackPort,
    ): AndroidPlaybackSession? = null

    override fun readVolumeKeySkipGestureEnabled(): Boolean = true

    override fun browseLibrary(): MediaBrowseLibrary = object : MediaBrowseLibrary {
        private fun track(id: Long) =
            BrowseTrack(id, "content://tree/$id.flac", "Track $id", "Artist", "Album", 1_000)

        override fun recentlyPlayed(limit: Int) = listOf(track(3), track(1))

        override fun playlists() = emptyList<BrowsePlaylist>()

        override fun playlistTracks(playlistId: Long) = emptyList<BrowseTrack>()

        override fun albums(offset: Int, limit: Int) = BrowsePage(emptyList<BrowseAlbum>(), false)

        override fun albumTracks(album: String, albumArtist: String) = emptyList<BrowseTrack>()

        override fun artists(offset: Int, limit: Int) = BrowsePage(emptyList<BrowseArtist>(), false)

        override fun artistAlbums(artist: String, offset: Int, limit: Int) =
            BrowsePage(emptyList<BrowseAlbum>(), false)
    }

    override fun resolveTrackMetadata(uri: String): TrackMetadata? = null

    override fun resolveArtworkPath(trackUri: String): String? = null

    override fun playTrackIds(trackIds: List<Long>, startIndex: Int) {
        playRequests += trackIds to startIndex
        check(!failPlayback) { "the Core refused" }
    }
}
