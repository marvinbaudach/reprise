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
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

private const val OWN = "io.github.marvinbaudach.reprise"
private const val AWAIT_LIMIT_MS = 10_000L

class BrowseTrustPolicyTest {
    @Test
    fun theAppItselfIsTrusted() {
        assertTrue(isTrustedBrowser(OWN, platformTrusted = false, ownPackage = OWN))
    }

    @Test
    fun aControllerThePlatformVouchesForIsTrusted() {
        assertTrue(isTrustedBrowser("com.android.systemui", platformTrusted = true, ownPackage = OWN))
    }

    @Test
    fun androidAutoAndWearAreTrustedEvenWhenThePlatformDoesNotSayso() {
        listOf(
            "com.google.android.projection.gearhead",
            "com.google.android.gms",
            "com.google.android.wearable.app",
        ).forEach { pkg -> assertTrue(pkg, isTrustedBrowser(pkg, platformTrusted = false, ownPackage = OWN)) }
    }

    @Test
    fun anyOtherInstalledAppIsNot() {
        assertFalse(isTrustedBrowser("com.example.snoop", platformTrusted = false, ownPackage = OWN))
        assertFalse(isTrustedBrowser("$OWN.evil", platformTrusted = false, ownPackage = OWN))
        assertFalse(isTrustedBrowser("com.google.android.gms.fake", platformTrusted = false, ownPackage = OWN))
    }
}

/** An untrusted controller gets no library; a trusted one gets the tree. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class BrowseTrustSessionTest {
    private val context: Context = ApplicationProvider.getApplicationContext()
    private val library = FixtureBrowseLibrary(
        recently = listOf(browseTrack(3), browseTrack(1)),
    )
    private val playRequests = mutableListOf<Pair<List<Long>, Int>>()
    private val exoPlayer = ExoPlayer.Builder(context).build()
    private val opened = mutableListOf<AutoCloseable>()

    @After
    fun release() {
        opened.reversed().forEach(AutoCloseable::close)
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

    private fun browser(trusted: Boolean, id: String): MediaBrowser {
        val callback = BrowseCallback(
            tree = { MediaBrowseTree(library, TEST_LABELS) },
            ownPackage = "someone.else",
            trust = { _, _, _ -> trusted },
        )
        val session = MediaLibrarySession.Builder(
            context,
            BrowsePlayer(exoPlayer) { ids, start -> playRequests += ids to start },
            callback,
        ).setId(id).build()
        val browser = await(MediaBrowser.Builder(context, session.token).buildAsync())
        opened += AutoCloseable { callback.close() }
        opened += AutoCloseable { session.release() }
        opened += AutoCloseable { browser.release() }
        return browser
    }

    @Test
    fun anUntrustedControllerGetsNoRootNoChildrenAndNoItems() {
        val browser = browser(trusted = false, id = "untrusted-read")

        val root = await(browser.getLibraryRoot(null))
        val children = await(browser.getChildren("recent", 0, 10, null))
        val item = await(browser.getItem("playlists"))

        assertTrue(root.resultCode != LibraryResult.RESULT_SUCCESS)
        assertTrue(children.resultCode != LibraryResult.RESULT_SUCCESS)
        assertTrue(item.resultCode != LibraryResult.RESULT_SUCCESS)
        assertEquals(null, children.value)
    }

    @Test
    fun anUntrustedControllerCannotPlayALibrarySong() {
        val browser = browser(trusted = false, id = "untrusted-play")
        val song = MediaItem.Builder()
            .setMediaId(BrowseId.Track(BrowseId.RecentlyPlayed, 3).mediaId)
            .setUri("content://tree/3.flac")
            .build()

        browser.setMediaItem(song)
        browser.prepare()
        // Two round trips behind the request: it has been answered by now.
        await(browser.getItem("recent"))
        await(browser.getItem("recent"))
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(emptyList<Pair<List<Long>, Int>>(), playRequests)
        assertEquals(0, exoPlayer.mediaItemCount)
    }

    @Test
    fun anUntrustedControllerKeepsTheTransport() {
        val browser = browser(trusted = false, id = "untrusted-transport")

        assertTrue(browser.isCommandAvailable(androidx.media3.common.Player.COMMAND_PLAY_PAUSE))
    }

    @Test
    fun aTrustedControllerGetsTheTree() {
        val browser = browser(trusted = true, id = "trusted-read")

        val children = await(browser.getChildren("recent", 0, 10, null))

        assertEquals(LibraryResult.RESULT_SUCCESS, children.resultCode)
        assertEquals(
            listOf("Track 3", "Track 1"),
            children.value!!.map { it.mediaMetadata.title.toString() },
        )
    }
}
