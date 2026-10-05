package io.github.marvinbaudach.reprise.library

import androidx.media3.common.MediaItem
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

private fun songItem(container: BrowseId, id: Long): MediaItem =
    MediaItem.Builder().setMediaId(BrowseId.Track(container, id).mediaId).build()

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class BrowsePlayerTest {
    private val inner = ItemListPlayer()
    private val requests = mutableListOf<Pair<List<Long>, Int>>()
    private val player = BrowsePlayer(inner.player) { ids, start -> requests += ids to start }
    private val album = BrowseId.Album("A", "B")

    @Test
    fun browseSongsBecomeACorePlayRequestInsteadOfReachingExoPlayer() {
        player.setMediaItems(
            listOf(songItem(album, 1), songItem(album, 2), songItem(album, 3)),
            1,
            0L,
        )

        assertEquals(listOf(listOf(1L, 2L, 3L) to 1), requests)
        assertEquals(emptyList<MediaItem>(), inner.items)
        assertFalse(inner.calls.contains("setMediaItems"))
    }

    @Test
    fun everySetterOverloadIsInterceptedForBrowseSongs() {
        val one = songItem(album, 4)

        player.setMediaItem(one)
        player.setMediaItem(one, 10L)
        player.setMediaItem(one, true)
        player.setMediaItems(listOf(one))
        player.setMediaItems(listOf(one), false)

        assertEquals(List(5) { listOf(4L) to 0 }, requests)
        assertTrue(inner.items.isEmpty())
    }

    @Test
    fun anUnsetStartIndexStartsAtTheFirstSong() {
        player.setMediaItems(listOf(songItem(album, 1), songItem(album, 2)), -1, 0L)

        assertEquals(listOf(listOf(1L, 2L) to 0), requests)
    }

    @Test
    fun anOutOfRangeStartIndexIsClampedIntoTheQueue() {
        player.setMediaItems(listOf(songItem(album, 1), songItem(album, 2)), 9, 0L)

        assertEquals(listOf(listOf(1L, 2L) to 1), requests)
    }

    @Test
    fun appendingBrowseSongsIsSwallowedBecauseTheCoreOwnsTheQueue() {
        player.addMediaItem(songItem(album, 1))
        player.addMediaItem(0, songItem(album, 1))
        player.addMediaItems(listOf(songItem(album, 1)))
        player.addMediaItems(0, listOf(songItem(album, 1)))

        assertTrue(inner.items.isEmpty())
        assertTrue(requests.isEmpty())
    }

    @Test
    fun anyOtherItemIsForwardedUntouched() {
        val own = MediaItem.Builder().setMediaId("42").setUri("content://tree/42.flac").build()

        player.setMediaItem(own)
        player.addMediaItem(own)

        assertEquals(listOf(own, own), inner.items)
        assertTrue(requests.isEmpty())
    }

    @Test
    fun aMixedListIsForwardedWholeNotHalfSwallowed() {
        val own = MediaItem.Builder().setMediaId("42").setUri("content://tree/42.flac").build()

        player.setMediaItems(listOf(own, songItem(album, 1)))

        assertEquals(2, inner.items.size)
        assertTrue(requests.isEmpty())
    }

    @Test
    fun anEmptyListIsForwarded() {
        player.setMediaItems(emptyList())

        assertTrue(inner.calls.contains("setMediaItems"))
        assertTrue(requests.isEmpty())
    }
}
