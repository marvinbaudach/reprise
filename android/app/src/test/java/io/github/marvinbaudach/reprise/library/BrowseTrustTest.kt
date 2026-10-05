package io.github.marvinbaudach.reprise.library

import android.content.Context
import android.os.Bundle
import android.os.Looper
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import androidx.media3.common.Player
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.session.LibraryResult
import androidx.media3.session.MediaBrowser
import androidx.media3.session.MediaLibraryService.MediaLibrarySession
import androidx.media3.session.MediaSession
import androidx.media3.session.SessionCommand
import androidx.media3.session.SessionError
import androidx.test.core.app.ApplicationProvider
import com.google.common.util.concurrent.ListenableFuture
import java.util.concurrent.ExecutionException
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

private const val OWN_UID = 10_100
private const val STRANGER_UID = 10_200
private const val AUTO = "com.google.android.projection.gearhead"
private val AUTO_RELEASE_KEY = "fdb00c43dbde8b51cb312aa81d3b5fa17713adb94b28f598d77f8eb89daceedf"
private const val AWAIT_LIMIT_MS = 10_000L

class BrowseTrustPolicyTest {
    private fun allowed(
        packageName: String,
        uid: Int = STRANGER_UID,
        platformTrusted: Boolean = false,
        signers: Set<String>? = null,
    ) = isAllowedBrowser(packageName, uid, platformTrusted, OWN_UID) { _, _ -> signers }

    @Test
    fun theAppItselfIsTrustedByUid() {
        assertTrue(allowed("io.github.marvinbaudach.reprise", uid = OWN_UID))
    }

    @Test
    fun aPackageNameIsNeverEnoughToPassForTheApp() {
        assertFalse(allowed("io.github.marvinbaudach.reprise", uid = STRANGER_UID))
    }

    @Test
    fun aControllerThePlatformVouchesForIsTrusted() {
        assertTrue(allowed("com.android.systemui", platformTrusted = true))
    }

    @Test
    fun androidAutoIsTrustedOnlyWithItsPinnedCertificate() {
        assertTrue(allowed(AUTO, signers = setOf(AUTO_RELEASE_KEY)))
    }

    @Test
    fun anAppClaimingAnAutoPackageNameWithAnotherCertificateIsRefused() {
        assertFalse(allowed(AUTO, signers = setOf("00".repeat(32))))
    }

    @Test
    fun aPinnedPackageWhoseUidDoesNotOwnItIsRefused() {
        // `signersOf` answers null when the claimed package is not owned by the caller's uid.
        assertFalse(allowed(AUTO, signers = null))
    }

    @Test
    fun aPinnedPackageSignedByTwoCertificatesIsRefused() {
        assertFalse(allowed(AUTO, signers = setOf(AUTO_RELEASE_KEY, "00".repeat(32))))
        assertFalse(allowed(AUTO, signers = emptySet()))
    }

    @Test
    fun anyOtherInstalledAppIsRefusedWithoutLookingAtItsCertificate() {
        var looked = false
        val result = isAllowedBrowser("com.example.snoop", STRANGER_UID, false, OWN_UID) { _, _ ->
            looked = true
            setOf(AUTO_RELEASE_KEY)
        }

        assertFalse(result)
        assertFalse(looked)
        assertFalse(allowed("com.google.android.gms", signers = setOf(AUTO_RELEASE_KEY)))
    }

    @Test
    fun theCertificateDigestIsASha256HexOfTheCertificateBytes() {
        assertEquals(
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            certificateDigest("abc".toByteArray()),
        )
    }

    @Test
    fun everyPinIsAFullSha256InLowercaseHex() {
        PINNED_SIGNERS.values.flatten().forEach { pin ->
            assertTrue(pin, Regex("[0-9a-f]{64}").matches(pin))
        }
    }
}

/** An untrusted controller gets no library through any entry point; a trusted one gets the tree. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class BrowseTrustSessionTest {
    private val context: Context = ApplicationProvider.getApplicationContext()
    private val library = FixtureBrowseLibrary(
        recently = listOf(browseTrack(3), browseTrack(1)),
        playlistList = listOf(BrowsePlaylist(10, "Secret Playlist", 1)),
    )
    private val playRequests = mutableListOf<Pair<List<Long>, Int>>()
    private val exoPlayer = ExoPlayer.Builder(context).build()
    private val opened = mutableListOf<AutoCloseable>()

    /** What the access decision answers right now; a test can flip it after connecting. */
    private var allowed = false

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

    private fun browser(connectedAs: Boolean, id: String): MediaBrowser {
        allowed = connectedAs
        val callback = BrowseCallback(
            tree = { MediaBrowseTree(library, TEST_LABELS) },
            access = BrowserAccess { allowed },
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

    private fun secretItem(uri: String, title: String) = MediaItem.Builder()
        .setMediaId(uri)
        .setUri(uri)
        .setMediaMetadata(MediaMetadata.Builder().setTitle(title).build())
        .build()

    private fun sessionOnPlayer(): MediaLibrarySession {
        val session = MediaLibrarySession.Builder(context, exoPlayer, object : MediaLibrarySession.Callback {})
            .setId("trust-test-${opened.size}")
            .build()
        opened += AutoCloseable { session.release() }
        return session
    }

    private val leaf: MediaItem
        get() = MediaItem.Builder()
            .setMediaId(BrowseId.Track(BrowseId.RecentlyPlayed, 3).mediaId)
            .setUri("content://tree/3.flac")
            .build()

    private fun settle(browser: MediaBrowser) {
        // Two round trips behind a request: it has been answered by now.
        await(browser.getItem("recent"))
        await(browser.getItem("recent"))
        shadowOf(Looper.getMainLooper()).idle()
    }

    @Test
    fun anUntrustedControllerIsToldItHasNoLibraryCommands() {
        val browser = browser(connectedAs = false, id = "untrusted-commands")

        val commands = browser.availableSessionCommands
        assertFalse(commands.contains(SessionCommand.COMMAND_CODE_LIBRARY_GET_LIBRARY_ROOT))
        assertFalse(commands.contains(SessionCommand.COMMAND_CODE_LIBRARY_GET_CHILDREN))
        assertFalse(commands.contains(SessionCommand.COMMAND_CODE_LIBRARY_GET_ITEM))
        assertFalse(commands.contains(SessionCommand.COMMAND_CODE_LIBRARY_SEARCH))
        assertFalse(commands.contains(SessionCommand.COMMAND_CODE_LIBRARY_SUBSCRIBE))
    }

    @Test
    fun anUntrustedControllerGetsNoBrowseDataFromAnyReadEntryPoint() {
        val browser = browser(connectedAs = false, id = "untrusted-reads")

        val results: List<LibraryResult<*>> = listOf(
            await(browser.getLibraryRoot(null)),
            await(browser.getChildren("playlists", 0, 10, null)),
            await(browser.getItem("playlists")),
            await(browser.search("secret", null)),
            await(browser.getSearchResult("secret", 0, 10, null)),
            await(browser.subscribe("playlists", null)),
            await(browser.unsubscribe("playlists")),
        )

        results.forEach { result ->
            assertTrue("${result.resultCode}", result.resultCode != LibraryResult.RESULT_SUCCESS)
            assertNull(result.value)
        }
    }

    @Test
    fun aControllerThatLosesTrustAfterConnectingIsRefusedByEveryEntryPoint() {
        val browser = browser(connectedAs = true, id = "revoked-reads")
        allowed = false

        val results: List<LibraryResult<*>> = listOf(
            await(browser.getLibraryRoot(null)),
            await(browser.getChildren("playlists", 0, 10, null)),
            await(browser.getItem("playlists")),
            await(browser.search("secret", null)),
            await(browser.getSearchResult("secret", 0, 10, null)),
            await(browser.subscribe("playlists", null)),
            await(browser.unsubscribe("playlists")),
        )

        results.forEach { result ->
            assertEquals(SessionError.ERROR_PERMISSION_DENIED, result.resultCode)
            assertNull(result.value)
        }
    }

    @Test
    fun aControllerThatLosesTrustCannotPlayALibrarySongThroughSetOrAdd() {
        val browser = browser(connectedAs = true, id = "revoked-play")
        allowed = false

        browser.setMediaItem(leaf)
        browser.addMediaItem(leaf)
        browser.addMediaItems(listOf(leaf))
        settle(browser)

        assertEquals(emptyList<Pair<List<Long>, Int>>(), playRequests)
        assertEquals(0, exoPlayer.mediaItemCount)
    }

    @Test
    fun anUntrustedControllerCannotPlayALibrarySong() {
        val browser = browser(connectedAs = false, id = "untrusted-play")

        browser.setMediaItem(leaf)
        browser.prepare()
        settle(browser)

        assertEquals(emptyList<Pair<List<Long>, Int>>(), playRequests)
        assertEquals(0, exoPlayer.mediaItemCount)
    }

    @Test
    fun anUntrustedControllerCannotSendACustomCommand() {
        val browser = browser(connectedAs = false, id = "untrusted-custom")

        val result = await(browser.sendCustomCommand(SessionCommand("probe", Bundle.EMPTY), Bundle.EMPTY))

        assertTrue(result.resultCode != 0)
    }

    @Test
    fun anUntrustedControllerIsGivenNoPlayerCommandAtAll() {
        val browser = browser(connectedAs = false, id = "untrusted-player-commands")

        listOf(
            Player.COMMAND_PLAY_PAUSE,
            Player.COMMAND_STOP,
            Player.COMMAND_SEEK_IN_CURRENT_MEDIA_ITEM,
            Player.COMMAND_SET_MEDIA_ITEM,
            Player.COMMAND_CHANGE_MEDIA_ITEMS,
            Player.COMMAND_SET_SHUFFLE_MODE,
            Player.COMMAND_SET_SPEED_AND_PITCH,
            Player.COMMAND_SET_DEVICE_VOLUME_WITH_FLAGS,
            Player.COMMAND_GET_TIMELINE,
            Player.COMMAND_GET_CURRENT_MEDIA_ITEM,
            Player.COMMAND_GET_METADATA,
        ).forEach { command ->
            assertFalse("command $command", browser.isCommandAvailable(command))
        }
        assertEquals(emptySet<SessionCommand>(), browser.availableSessionCommands.commands)
    }

    @Test
    fun anUntrustedControllerSeesNeitherTheCurrentItemNorTheTimeline() {
        exoPlayer.setMediaItems(
            listOf(
                secretItem("file:///music/secret-1.flac", "Secret Song"),
                secretItem("file:///music/secret-2.flac", "Secret Next"),
            ),
        )
        val browser = browser(connectedAs = false, id = "untrusted-snoop")
        settle(browser)

        assertNull(browser.currentMediaItem)
        assertTrue(browser.currentTimeline.isEmpty)
        assertEquals(0, browser.mediaItemCount)
        assertEquals(MediaMetadata.EMPTY, browser.mediaMetadata)
        assertEquals(MediaMetadata.EMPTY, browser.playlistMetadata)
    }

    @Test
    fun aTrustedControllerStillSeesTheCurrentItem() {
        exoPlayer.setMediaItem(secretItem("file:///music/ok.flac", "Open Song"))
        val browser = browser(connectedAs = true, id = "trusted-current")
        settle(browser)

        assertEquals("Open Song", browser.currentMediaItem?.mediaMetadata?.title)
        assertTrue(browser.isCommandAvailable(Player.COMMAND_PLAY_PAUSE))
    }

    @Test
    fun aControllerThatLosesTrustCannotDriveThePlayer() {
        exoPlayer.playWhenReady = true
        val browser = browser(connectedAs = true, id = "revoked-transport")
        allowed = false

        browser.pause()
        browser.seekTo(5_000)
        browser.setMediaItem(MediaItem.fromUri("http://example.invalid/stream"))
        browser.setMediaItems(listOf(MediaItem.fromUri("file:///etc/passwd")))
        browser.addMediaItem(MediaItem.fromUri("content://other.app/secret"))
        browser.stop()
        browser.setShuffleModeEnabled(true)
        browser.setPlaybackSpeed(2f)
        settle(browser)

        assertTrue(exoPlayer.playWhenReady)
        assertEquals(0, exoPlayer.mediaItemCount)
        assertFalse(exoPlayer.shuffleModeEnabled)
        assertEquals(1f, exoPlayer.playbackParameters.speed, 0f)
    }

    @Test
    fun aRefusedPlayerCommandIsAnsweredWithPermissionDenied() {
        val callback = BrowseCallback(
            tree = { MediaBrowseTree(library, TEST_LABELS) },
            access = BrowserAccess { false },
        )
        opened += AutoCloseable { callback.close() }
        val stranger = MediaSession.ControllerInfo.createTestOnlyControllerInfo(
            "com.example.snoop", 1, STRANGER_UID, 1, 1, false, Bundle.EMPTY, false,
        )

        assertEquals(
            SessionError.ERROR_PERMISSION_DENIED,
            callback.onPlayerCommandRequest(sessionOnPlayer(), stranger, Player.COMMAND_PLAY_PAUSE),
        )
    }

    @Test
    fun aControllerThatMayNotBrowseCannotSmuggleInAUriItem() {
        val callback = BrowseCallback(
            tree = { MediaBrowseTree(library, TEST_LABELS) },
            access = BrowserAccess { false },
        )
        opened += AutoCloseable { callback.close() }
        val stranger = MediaSession.ControllerInfo.createTestOnlyControllerInfo(
            "com.example.snoop", 1, STRANGER_UID, 1, 1, false, Bundle.EMPTY, false,
        )
        val uriItem = MediaItem.fromUri("file:///etc/passwd")

        assertThrows(ExecutionException::class.java) {
            await(callback.onAddMediaItems(sessionOnPlayer(), stranger, listOf(uriItem)))
        }
        assertThrows(ExecutionException::class.java) {
            await(callback.onSetMediaItems(sessionOnPlayer(), stranger, listOf(uriItem), 0, 0))
        }
    }

    @Test
    fun aTrustedControllerStillCanSetAnItemWithAUri() {
        val callback = BrowseCallback(
            tree = { MediaBrowseTree(library, TEST_LABELS) },
            access = BrowserAccess { true },
        )
        opened += AutoCloseable { callback.close() }
        val friend = MediaSession.ControllerInfo.createTestOnlyControllerInfo(
            "com.example.friend", 1, STRANGER_UID, 1, 1, true, Bundle.EMPTY, false,
        )
        val uriItem = MediaItem.fromUri("file:///music/a.flac")

        assertEquals(listOf(uriItem), await(callback.onAddMediaItems(sessionOnPlayer(), friend, listOf(uriItem))))
    }

    @Test
    fun aTrustedControllerGetsTheTree() {
        val browser = browser(connectedAs = true, id = "trusted-read")

        val children = await(browser.getChildren("recent", 0, 10, null))

        assertEquals(LibraryResult.RESULT_SUCCESS, children.resultCode)
        assertEquals(
            listOf("Track 3", "Track 1"),
            children.value!!.map { it.mediaMetadata.title.toString() },
        )
        assertTrue(browser.availableSessionCommands.contains(SessionCommand.COMMAND_CODE_LIBRARY_GET_CHILDREN))
    }

    @Test
    fun aTrustedControllerAskingForUnsupportedLibraryFeaturesIsNotToldItIsForbidden() {
        val browser = browser(connectedAs = true, id = "trusted-unsupported")

        val search = await(browser.search("x", null))

        assertEquals(SessionError.ERROR_NOT_SUPPORTED, search.resultCode)
    }
}
