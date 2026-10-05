package io.github.marvinbaudach.reprise.library

import android.content.Context
import android.os.Looper
import androidx.media3.common.MediaItem
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.session.LibraryResult
import androidx.media3.session.MediaBrowser
import androidx.media3.session.MediaLibraryService.MediaLibrarySession
import androidx.test.core.app.ApplicationProvider
import com.google.common.util.concurrent.ListenableFuture
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

private const val AWAIT_LIMIT_MS = 10_000L

/**
 * The browse tree through a real session and a real [MediaBrowser], so the
 * parts that only exist inside Media3 are exercised too: that the callback is
 * wired, that a leaf tapped by a browser reaches the Core as its whole
 * container, and that ExoPlayer is never handed the browse items.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class BrowseSessionTest {
    private val context: Context = ApplicationProvider.getApplicationContext()
    private val library = FixtureBrowseLibrary(
        recently = listOf(browseTrack(3), browseTrack(1)),
        playlistList = listOf(BrowsePlaylist(10, "Road", 3)),
        playlistContents = mapOf(10L to listOf(browseTrack(1), browseTrack(2), browseTrack(3))),
        albumList = listOf(BrowseAlbum("First Album", "Alpha", 2)),
        albumContents = mapOf(("First Album" to "Alpha") to listOf(browseTrack(1), browseTrack(2))),
    )
    private val playRequests = mutableListOf<Pair<List<Long>, Int>>()
    private lateinit var exoPlayer: ExoPlayer
    private lateinit var callback: BrowseCallback
    private lateinit var session: MediaLibrarySession
    private lateinit var browser: MediaBrowser

    @Before
    fun connect() {
        exoPlayer = ExoPlayer.Builder(context).build()
        callback = BrowseCallback(
            tree = { MediaBrowseTree(library, TEST_LABELS) },
            ownPackage = context.packageName,
        )
        session = MediaLibrarySession.Builder(
            context,
            BrowsePlayer(exoPlayer) { ids, start -> playRequests += ids to start },
            callback,
        ).setId("browse-session-test").build()
        browser = await(MediaBrowser.Builder(context, session.token).buildAsync())
    }

    @After
    fun disconnect() {
        browser.release()
        session.release()
        callback.close()
        exoPlayer.release()
    }

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

    private fun titlesOf(result: LibraryResult<com.google.common.collect.ImmutableList<MediaItem>>) =
        result.value!!.map { it.mediaMetadata.title.toString() }

    @Test
    fun aBrowserSeesTheRootAndItsFourFolders() {
        val root = await(browser.getLibraryRoot(null))
        val children = await(browser.getChildren(root.value!!.mediaId, 0, 10, null))

        assertEquals("Reprise", root.value!!.mediaMetadata.title)
        assertEquals(
            listOf("Recently played", "Playlists", "Albums", "Artists"),
            titlesOf(children),
        )
    }

    @Test
    fun aBrowserWalksFromAFolderDownToItsSongs() {
        val playlists = await(browser.getChildren("playlists", 0, 10, null))
        val road = playlists.value!!.single()
        val songs = await(browser.getChildren(road.mediaId, 0, 10, null))

        assertEquals(listOf("Track 1", "Track 2", "Track 3"), titlesOf(songs))
    }

    @Test
    fun aBrowserCanPageThroughAFolder() {
        val second = await(browser.getChildren(BrowseId.Playlist(10).mediaId, 1, 2, null))

        assertEquals(listOf("Track 3"), titlesOf(second))
    }

    @Test
    fun anUnknownParentIsAnErrorNotAnEmptyFolder() {
        val result = await(browser.getChildren("nonsense", 0, 10, null))

        assertTrue(result.resultCode != LibraryResult.RESULT_SUCCESS)
    }

    @Test
    fun theRecentRootIsRefused() {
        val params = androidx.media3.session.MediaLibraryService.LibraryParams.Builder()
            .setRecent(true)
            .build()

        val result = await(browser.getLibraryRoot(params))

        assertTrue(result.resultCode != LibraryResult.RESULT_SUCCESS)
    }

    @Test
    fun aSongTappedInABrowserReachesTheCoreAsItsWholeContainer() {
        val songs = await(browser.getChildren(BrowseId.Playlist(10).mediaId, 0, 10, null))
        val tapped = songs.value!![1]

        browser.setMediaItem(tapped)
        browser.prepare()
        browser.play()
        awaitUntil { playRequests.isNotEmpty() }

        assertEquals(listOf(listOf(1L, 2L, 3L) to 1), playRequests)
        assertEquals(
            "ExoPlayer must never be handed a browse item; the Core owns the queue",
            0,
            exoPlayer.mediaItemCount,
        )
    }

    @Test
    fun aSongFromRecentlyPlayedQueuesTheRecentList() {
        val songs = await(browser.getChildren("recent", 0, 10, null))

        browser.setMediaItem(songs.value!![1])
        awaitUntil { playRequests.isNotEmpty() }

        assertEquals(listOf(listOf(3L, 1L) to 1), playRequests)
    }

    @Test
    fun anItemCanBeLookedUpByItsId() {
        val item = await(browser.getItem(BrowseId.Album("First Album", "Alpha").mediaId))

        assertEquals("First Album", item.value!!.mediaMetadata.title)
    }
}
