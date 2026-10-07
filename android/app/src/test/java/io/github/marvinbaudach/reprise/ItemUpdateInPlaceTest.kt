package io.github.marvinbaudach.reprise

import android.net.Uri
import androidx.annotation.OptIn
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import androidx.media3.common.Player
import androidx.media3.common.Timeline
import androidx.media3.common.util.UnstableApi
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.exoplayer.source.DefaultMediaSourceFactory
import io.github.marvinbaudach.reprise.library.PlaybackItems
import io.github.marvinbaudach.reprise.library.PlaybackKey
import io.github.marvinbaudach.reprise.library.PlaybackRequest
import io.github.marvinbaudach.reprise.library.TrackMetadata
import io.github.marvinbaudach.reprise.library.TrackMetadataResolver
import io.github.marvinbaudach.reprise.library.playbackMediaItem
import androidx.test.core.app.ApplicationProvider
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.reprise_android_ffi.AndroidPlaybackSegment

/**
 * A late cover is attached by replacing the playing item with one that only
 * differs in its metadata. That is cheap only if ExoPlayer can update the item
 * in place; if it could not, every track would restart when its cover arrived.
 *
 * Two checks, because each proves half: the source says it can take the new
 * item, and the player, asked through `replaceMediaItem` (the call the port
 * makes), keeps the playlist entry instead of removing and adding one.
 */
@OptIn(UnstableApi::class)
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class ItemUpdateInPlaceTest {
    private val context = ApplicationProvider.getApplicationContext<android.content.Context>()
    private val uri = Uri.parse("content://tree/1.flac")
    private val before = MediaItem.Builder().setUri(uri).setMediaId("1")
        .setMediaMetadata(MediaMetadata.Builder().setTitle("Song").build())
        .build()
    private val after = before.buildUpon()
        .setMediaMetadata(
            before.mediaMetadata.buildUpon().setArtworkUri(Uri.parse("file:///cache/1.png")).build(),
        )
        .build()
    private val player = ExoPlayer.Builder(context).build()

    @After
    fun release() {
        player.release()
    }

    @Test
    fun anItemThatGainsACoverCanBeUpdatedWithoutRebuildingItsSource() {
        val source = DefaultMediaSourceFactory(context).createMediaSource(before)

        assertTrue(source.canUpdateMediaItem(after))
    }

    @Test
    fun replacingTheItemThroughThePlayerKeepsItsPlaylistEntryAndStartsNothing() {
        player.setMediaItems(listOf(before, MediaItem.fromUri("content://tree/2.flac")))
        val window = Timeline.Window()
        val entryBefore = player.currentTimeline.getWindow(0, window).uid
        val transitions = mutableListOf<Int>()
        val discontinuities = mutableListOf<Int>()
        player.addListener(object : Player.Listener {
            override fun onMediaItemTransition(mediaItem: MediaItem?, reason: Int) {
                transitions += reason
            }

            override fun onPositionDiscontinuity(
                oldPosition: Player.PositionInfo,
                newPosition: Player.PositionInfo,
                reason: Int,
            ) {
                discontinuities += reason
            }
        })

        player.replaceMediaItem(0, after)

        assertEquals(after, player.getMediaItemAt(0))
        assertEquals(2, player.mediaItemCount)
        assertEquals(entryBefore, player.currentTimeline.getWindow(0, window).uid)
        assertEquals(emptyList<Int>(), discontinuities)
        assertTrue("transitions: $transitions", transitions.all { it == Player.MEDIA_ITEM_TRANSITION_REASON_PLAYLIST_CHANGED })
    }

    @Test
    fun aBareItemCompletedWithIdMetadataAndCoverKeepsItsEntryToo() {
        // The port starts an unknown track bare and completes it a moment later,
        // with another media id as well: that replacement must not restart it.
        val bare = MediaItem.Builder().setUri(uri).build()
        val full = playbackMediaItem(
            uri.toString(),
            TrackMetadata(1, "Song", "Singer", "Album", 1_000, Uri.parse("file:///cache/1.png")),
            mediaId = "track:recent:1:4",
        )
        player.setMediaItems(listOf(bare, MediaItem.fromUri("content://tree/2.flac")))
        val window = Timeline.Window()
        val entryBefore = player.currentTimeline.getWindow(0, window).uid
        val discontinuities = mutableListOf<Int>()
        player.addListener(object : Player.Listener {
            override fun onPositionDiscontinuity(
                oldPosition: Player.PositionInfo,
                newPosition: Player.PositionInfo,
                reason: Int,
            ) {
                discontinuities += reason
            }
        })

        player.replaceMediaItem(0, full)

        assertEquals(full, player.getMediaItemAt(0))
        assertEquals(entryBefore, player.currentTimeline.getWindow(0, window).uid)
        assertEquals(emptyList<Int>(), discontinuities)
    }

    @Test
    fun mtp_66_a_clipped_cue_track_that_gains_its_cover_is_updated_in_place() {
        // Built the way the port builds them: one clip per CUE track, the tag
        // carrying its request, the cover added once the file's cover is known.
        val items = PlaybackItems(TrackMetadataResolver { key ->
            TrackMetadata(key.trackId ?: 0, "Day of the Lords", "Joy Division", "", 10_000)
        })
        val request = PlaybackRequest(
            PlaybackKey(22, uri.toString()),
            AndroidPlaybackSegment(startMs = 10_000, endMs = 20_000),
        )
        items.resolve(request.key)
        val clipped = items.build(request)
        items.rememberCover(uri.toString(), Uri.parse("file:///cache/album.png"))
        val covered = items.build(request)
        assertTrue(DefaultMediaSourceFactory(context).createMediaSource(clipped).canUpdateMediaItem(covered))
        player.setMediaItems(listOf(clipped, MediaItem.fromUri("content://tree/2.flac")))
        val window = Timeline.Window()
        val entryBefore = player.currentTimeline.getWindow(0, window).uid

        player.replaceMediaItem(0, covered)

        assertEquals(covered, player.getMediaItemAt(0))
        assertEquals(entryBefore, player.currentTimeline.getWindow(0, window).uid)
    }

    @Test
    fun controlArmAnItemWithAnotherUriIsNotKeptSoTheEntryCheckCanFail() {
        player.setMediaItems(listOf(before, MediaItem.fromUri("content://tree/2.flac")))
        val window = Timeline.Window()
        val entryBefore = player.currentTimeline.getWindow(0, window).uid

        player.replaceMediaItem(0, after.buildUpon().setUri("content://tree/other.flac").build())

        assertTrue(entryBefore != player.currentTimeline.getWindow(0, window).uid)
    }
}
