package io.github.marvinbaudach.reprise

import android.net.Uri
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
import org.robolectric.annotation.Config
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

/** Notification, lock screen, Auto and the widget all read the item's metadata. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class Media3PlaybackPortMetadataTest {
    private val fake = ItemListPlayer()
    private val resolved = mutableListOf<String>()
    private var fail = false
    private val port = Media3PlaybackPort(
        fake.player,
        equalizerChanged = {},
        metadata = TrackMetadataResolver { uri ->
            resolved += uri
            if (fail) error("the library is not answering")
            mapOf(FIRST to FIRST_TRACK, SECOND to SECOND_TRACK)[uri]
        },
    )

    @After
    fun release() {
        port.release()
    }

    @Test
    fun theItemTheCoreStartsCarriesTheTracksMetadata() {
        port.playUri(FIRST)

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
    fun theGaplessNextItemCarriesItsMetadataToo() {
        port.playUri(FIRST)

        port.setNext(SECOND)

        assertEquals(listOf("First", "Second"), fake.items.map { it.mediaMetadata.title })
        assertEquals(listOf("11", "12"), fake.items.map { it.mediaId })
    }

    @Test
    fun aNextItemSetBeforeATransitionModeChangeIsNotResolvedAgain() {
        port.playUri(FIRST)
        port.setNext(SECOND)
        resolved.clear()

        port.setTransition(AndroidTransitionMode.GAPLESS)

        assertEquals(emptyList<String>(), resolved)
        assertEquals(listOf("First", "Second"), fake.items.map { it.mediaMetadata.title })
    }

    @Test
    fun aTrackTheLibraryDoesNotKnowStillPlaysWithoutMetadata() {
        port.playUri("content://tree/stranger.flac")

        val item = fake.items.single()
        assertEquals(Uri.parse("content://tree/stranger.flac"), item.localConfiguration?.uri)
        assertNull(item.mediaMetadata.title)
        assertTrue(fake.calls.contains("play"))
    }

    @Test
    fun aLibraryThatFailsDoesNotStopThePlayback() {
        fail = true

        port.playUri(FIRST)

        assertEquals(Uri.parse(FIRST), fake.items.single().localConfiguration?.uri)
        assertNull(fake.items.single().mediaMetadata.title)
        assertTrue(fake.calls.contains("play"))
    }

    @Test
    fun aLateCoverIsAttachedToTheMatchingItemOnly() {
        port.playUri(FIRST)
        port.setNext(SECOND)
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
        port.attachArtwork(FIRST, Uri.parse("file:///cache/first.png"))
        fake.calls.clear()

        port.attachArtwork(FIRST, Uri.parse("file:///cache/other.png"))

        assertFalse(fake.calls.contains("replaceMediaItem"))
        assertEquals(Uri.parse("file:///cache/first.png"), fake.items[0].mediaMetadata.artworkUri)
    }
}
