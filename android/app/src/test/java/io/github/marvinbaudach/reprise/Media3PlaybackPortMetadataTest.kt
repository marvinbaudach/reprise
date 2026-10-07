package io.github.marvinbaudach.reprise

import android.net.Uri
import android.os.Looper
import androidx.media3.common.MediaMetadata
import io.github.marvinbaudach.reprise.library.ItemListPlayer
import io.github.marvinbaudach.reprise.library.TrackMetadata
import io.github.marvinbaudach.reprise.library.TrackMetadataResolver
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import java.util.concurrent.Executor
import uniffi.reprise_android_ffi.AndroidPlaybackSegment
import uniffi.reprise_android_ffi.AndroidTransitionMode

private const val FIRST = "content://tree/first.flac"
private const val SECOND = "content://tree/second.flac"

private val FIRST_TRACK = TrackMetadata(
    trackId = 11,
    title = "First",
    artist = "An Artist",
    album = "An Album",
    durationMs = 123_000,
)
private val SECOND_TRACK = TrackMetadata(
    trackId = 12,
    title = "Second",
    artist = "An Artist",
    album = "An Album",
    durationMs = 200_000,
)

private const val ALBUM = "content://tree/album.flac"

/** Two tracks a CUE sheet cuts from [ALBUM], known by their rows. */
private val CUE_TRACKS = mapOf(
    21L to TrackMetadata(21, "Disorder", "Joy Division", "Unknown Pleasures", 10_000),
    22L to TrackMetadata(22, "Day of the Lords", "Joy Division", "Unknown Pleasures", 20_000),
)

private fun cueItem(trackId: Long, startMs: Long, endMs: Long?) = playbackItem(
    ALBUM,
    trackId = trackId,
    segment = AndroidPlaybackSegment(startMs = startMs, endMs = endMs),
)

/**
 * Notification, lock screen, Auto and the widget all read the item's metadata.
 *
 * The test thread is the player's application thread, where the port must not
 * read the library: an unknown track starts bare and the executor completes it.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class Media3PlaybackPortMetadataTest {
    private val fake = ItemListPlayer()
    private val resolved = mutableListOf<String>()
    private val queued = ArrayDeque<Runnable>()
    private var fail = false
    private var mediaIds: (Long) -> String? = { null }
    private val port = Media3PlaybackPort(
        fake.player,
        equalizerChanged = {},
        metadata = TrackMetadataResolver { key ->
            resolved += key.uri
            if (fail) error("the library is not answering")
            key.trackId?.let(CUE_TRACKS::get)
                ?: mapOf(FIRST to FIRST_TRACK, SECOND to SECOND_TRACK)[key.uri]
        },
        mediaIdOf = { trackId -> mediaIds(trackId) },
        metadataExecutor = Executor { queued.addLast(it) },
    )

    @After
    fun release() {
        port.release()
    }

    /** Runs the library reads the port queued, then the player-thread work they post back. */
    private fun settle() {
        while (queued.isNotEmpty()) queued.removeFirst().run()
        shadowOf(Looper.getMainLooper()).idle()
    }

    @Test
    fun os_11_the_item_the_core_starts_carries_the_tracks_metadata() {
        port.playUri(FIRST)
        settle()

        val item = fake.items.single()
        assertEquals(Uri.parse(FIRST), item.localConfiguration?.uri)
        assertEquals("11", item.mediaId)
        assertEquals("First", item.mediaMetadata.title)
        assertEquals("First", item.mediaMetadata.displayTitle)
        assertEquals("An Artist", item.mediaMetadata.artist)
        assertEquals("An Album", item.mediaMetadata.albumTitle)
        assertEquals(123_000L, item.mediaMetadata.durationMs)
        assertEquals(MediaMetadata.MEDIA_TYPE_MUSIC, item.mediaMetadata.mediaType)
        assertTrue(item.mediaMetadata.isPlayable == true)
        assertFalse(item.mediaMetadata.isBrowsable == true)
    }

    @Test
    fun thePlayersOwnThreadStartsTheTrackWithoutWaitingForTheLibrary() {
        port.playUri(FIRST)

        assertEquals(emptyList<String>(), resolved)
        assertEquals(Uri.parse(FIRST), fake.items.single().localConfiguration?.uri)
        assertNull(fake.items.single().mediaMetadata.title)
        assertTrue(fake.calls.contains("play"))
    }

    @Test
    fun theLateMetadataCompletesTheItemInPlaceWithoutRestartingIt() {
        port.playUri(FIRST)
        fake.calls.clear()

        settle()

        assertEquals(listOf("replaceMediaItem"), fake.calls.filter { it == "replaceMediaItem" })
        assertFalse(fake.calls.contains("setMediaItem"))
        assertFalse(fake.calls.contains("prepare"))
        assertEquals("First", fake.items.single().mediaMetadata.title)
    }

    @Test
    fun aCallerOffThePlayersThreadReadsTheLibraryAndStartsTheItemComplete() {
        val worker = Thread { port.playUri(FIRST) }
        worker.start()
        while (worker.isAlive) {
            shadowOf(Looper.getMainLooper()).idle()
            Thread.sleep(2)
        }
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(listOf(FIRST), resolved)
        assertEquals("First", fake.items.single().mediaMetadata.title)
        assertTrue(queued.isEmpty())
    }

    @Test
    fun theGaplessNextItemCarriesItsMetadataToo() {
        port.playUri(FIRST)

        port.setNext(playbackItem(SECOND))
        settle()

        assertEquals(listOf("First", "Second"), fake.items.map { it.mediaMetadata.title })
        assertEquals(listOf("11", "12"), fake.items.map { it.mediaId })
    }

    @Test
    fun aNextItemSetBeforeATransitionModeChangeIsNotResolvedAgain() {
        port.playUri(FIRST)
        port.setNext(playbackItem(SECOND))
        settle()
        resolved.clear()

        port.setTransition(AndroidTransitionMode.GAPLESS)
        settle()

        assertEquals(emptyList<String>(), resolved)
        assertEquals(listOf("First", "Second"), fake.items.map { it.mediaMetadata.title })
    }

    @Test
    fun aNextItemQueuedBeforeItsMetadataArrivedIsNotReAddedBare() {
        port.playUri(FIRST)
        port.setNext(playbackItem(SECOND))

        port.setTransition(AndroidTransitionMode.GAPLESS)
        settle()

        assertEquals(listOf("First", "Second"), fake.items.map { it.mediaMetadata.title })
    }

    @Test
    fun aTrackTheLibraryDoesNotKnowStillPlaysWithoutMetadata() {
        port.playUri("content://tree/stranger.flac")
        settle()

        val item = fake.items.single()
        assertEquals(Uri.parse("content://tree/stranger.flac"), item.localConfiguration?.uri)
        assertNull(item.mediaMetadata.title)
        assertTrue(fake.calls.contains("play"))
    }

    @Test
    fun aLibraryThatFailsDoesNotStopThePlayback() {
        fail = true

        port.playUri(FIRST)
        settle()

        assertEquals(Uri.parse(FIRST), fake.items.single().localConfiguration?.uri)
        assertNull(fake.items.single().mediaMetadata.title)
        assertTrue(fake.calls.contains("play"))
    }

    @Test
    fun aLateCoverIsAttachedToTheMatchingItemOnly() {
        port.playUri(FIRST)
        port.setNext(playbackItem(SECOND))
        settle()
        val cover = Uri.parse("file:///cache/first.png")

        port.attachArtwork(FIRST, cover)

        assertEquals(cover, fake.items[0].mediaMetadata.artworkUri)
        assertNull(fake.items[1].mediaMetadata.artworkUri)
        assertEquals("First", fake.items[0].mediaMetadata.title)
        assertEquals("11", fake.items[0].mediaId)
    }

    @Test
    fun aCoverAlreadyOnTheItemIsNotReplacedAgain() {
        port.playUri(FIRST)
        settle()
        port.attachArtwork(FIRST, Uri.parse("file:///cache/first.png"))
        fake.calls.clear()

        port.attachArtwork(FIRST, Uri.parse("file:///cache/other.png"))

        assertFalse(fake.calls.contains("replaceMediaItem"))
        assertEquals(Uri.parse("file:///cache/first.png"), fake.items[0].mediaMetadata.artworkUri)
    }

    @Test
    fun aTrackPlayedAgainStartsWithItsCoverAndAsksNothing() {
        port.playUri(FIRST)
        settle()
        port.attachArtwork(FIRST, Uri.parse("file:///cache/first.png"))
        resolved.clear()

        port.playUri(FIRST)

        assertEquals(Uri.parse("file:///cache/first.png"), fake.items.single().mediaMetadata.artworkUri)
        assertEquals("First", fake.items.single().mediaMetadata.title)
        assertEquals(emptyList<String>(), resolved)
        assertTrue(queued.isEmpty())
    }

    @Test
    fun aGaplessNextItemOfAKnownTrackIsBornWithItsCover() {
        port.playUri(SECOND)
        settle()
        port.attachArtwork(SECOND, Uri.parse("file:///cache/second.png"))
        port.playUri(FIRST)
        settle()

        port.setNext(playbackItem(SECOND))

        assertEquals(
            Uri.parse("file:///cache/second.png"),
            fake.items.last().mediaMetadata.artworkUri,
        )
    }

    @Test
    fun aCoverThatArrivesBeforeTheMetadataIsNotLost() {
        port.playUri(FIRST)
        port.attachArtwork(FIRST, Uri.parse("file:///cache/first.png"))

        settle()

        assertEquals("First", fake.items.single().mediaMetadata.title)
        assertEquals(Uri.parse("file:///cache/first.png"), fake.items.single().mediaMetadata.artworkUri)
    }

    @Test
    fun theItemThatPlaysCarriesTheIdTheBrowseTreeListedItUnder() {
        mediaIds = { trackId -> if (trackId == 11L) "track:recent:11:4" else null }

        port.playUri(FIRST)
        port.setNext(playbackItem(SECOND))
        settle()

        assertEquals(listOf("track:recent:11:4", "12"), fake.items.map { it.mediaId })
    }

    @Test
    fun aReleasedPortIgnoresAnAnswerThatArrivesLate() {
        port.playUri(FIRST)
        port.release()
        fake.calls.clear()

        settle()

        assertFalse(fake.calls.contains("replaceMediaItem"))
    }

    @Test
    fun mtp_66_two_tracks_of_one_cue_file_each_carry_their_own_metadata() {
        port.playPath(cueItem(21, 0, 10_000))
        port.setNext(cueItem(22, 10_000, null))
        settle()

        assertEquals(listOf("Disorder", "Day of the Lords"), fake.items.map { it.mediaMetadata.title })
        assertEquals(listOf("21", "22"), fake.items.map { it.mediaId })
        assertTrue(fake.items.all { it.localConfiguration?.uri == Uri.parse(ALBUM) })
    }

    @Test
    fun mtp_66_the_files_cover_reaches_each_of_its_tracks_without_swapping_them() {
        port.playPath(cueItem(21, 0, 10_000))
        port.setNext(cueItem(22, 10_000, null))
        settle()
        val cover = Uri.parse("file:///cache/album.png")

        port.attachArtwork(ALBUM, cover)

        assertEquals(listOf(cover, cover), fake.items.map { it.mediaMetadata.artworkUri })
        assertEquals(listOf("Disorder", "Day of the Lords"), fake.items.map { it.mediaMetadata.title })
    }
}
