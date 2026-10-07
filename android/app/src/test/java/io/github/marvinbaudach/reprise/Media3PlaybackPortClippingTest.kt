package io.github.marvinbaudach.reprise

import android.os.Looper
import androidx.media3.common.C
import androidx.media3.common.MediaItem
import androidx.media3.common.Player
import io.github.marvinbaudach.reprise.library.ItemListPlayer
import io.github.marvinbaudach.reprise.library.TrackMetadata
import io.github.marvinbaudach.reprise.library.TrackMetadataResolver
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import java.util.concurrent.Executor
import uniffi.reprise_android_ffi.AndroidPlaybackSegment

private const val ALBUM = "content://tree/album.flac"

private fun segment(startMs: Long, endMs: Long?) = AndroidPlaybackSegment(startMs = startMs, endMs = endMs)

private fun clipOf(item: MediaItem) =
    item.clippingConfiguration.startPositionMs to item.clippingConfiguration.endPositionMs

/**
 * A track cut from a CUE file plays as a clip of that file (MTP-66), and the
 * file's last track is clipped at its start only, so it plays to the end of
 * the audio whatever the metadata claims (MTP-68).
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class Media3PlaybackPortClippingTest {
    private val fake = ItemListPlayer()
    private val queued = ArrayDeque<Runnable>()
    private val port = Media3PlaybackPort(
        fake.player,
        equalizerChanged = {},
        metadata = TrackMetadataResolver { key ->
            key.trackId?.let { id -> TrackMetadata(id, "Track $id", "Joy Division", "", 0) }
        },
        metadataExecutor = Executor { queued.addLast(it) },
    )

    @After
    fun release() {
        port.release()
    }

    private fun settle() {
        while (queued.isNotEmpty()) queued.removeFirst().run()
        shadowOf(Looper.getMainLooper()).idle()
    }

    @Test
    fun mtp_66_a_cue_track_and_the_next_one_are_each_clipped_to_their_own_stretch() {
        port.playPath(playbackItem(ALBUM, trackId = 21, segment = segment(0, 10_000)))
        port.setNext(playbackItem(ALBUM, trackId = 22, segment = segment(10_000, 20_000)))

        assertEquals(listOf(0L to 10_000L, 10_000L to 20_000L), fake.items.map(::clipOf))
    }

    @Test
    fun mtp_68_the_last_track_of_a_file_is_clipped_at_its_start_only() {
        port.playPath(playbackItem(ALBUM, trackId = 23, segment = segment(20_000, null)))

        assertEquals(20_000L to C.TIME_END_OF_SOURCE, clipOf(fake.items.single()))
    }

    @Test
    fun mtp_66_a_whole_file_is_not_clipped() {
        port.playPath(playbackItem("content://tree/song.flac", trackId = 5))

        assertEquals(MediaItem.ClippingConfiguration.UNSET, fake.items.single().clippingConfiguration)
    }

    @Test
    fun mtp_66_late_metadata_completes_a_clip_in_place_and_keeps_its_stretch() {
        port.playPath(playbackItem(ALBUM, trackId = 22, segment = segment(10_000, 20_000)))
        fake.calls.clear()

        settle()

        assertEquals(listOf("replaceMediaItem"), fake.calls.filter { it == "replaceMediaItem" })
        assertFalse(fake.calls.contains("setMediaItem"))
        assertFalse(fake.calls.contains("prepare"))
        assertEquals("Track 22", fake.items.single().mediaMetadata.title)
        assertEquals(10_000L to 20_000L, clipOf(fake.items.single()))
    }

    // The gain sink keys on the stream offsets it is told about. The offsets
    // ExoPlayer announces for two clipped items of one file are not produced
    // here, so this proves the sink and the port, not the clips; that is the
    // device check named in MTP-66.
    @Test
    fun two_queued_items_of_one_uri_each_play_at_the_gain_of_their_own_offset() {
        val probe = RecordingAudioSink()
        val sink = TrackGainAudioSink(probe.sink)
        val fake = CallbackPlayer(playbackState = Player.STATE_IDLE, playWhenReady = false)
        val port = Media3PlaybackPort(fake.player, trackGainSink = sink) {}
        val boundaryUs = 10_000_000L
        port.setNext(playbackItem(ALBUM, gainDb = 6.020599913, trackId = 22, segment = segment(10_000, 20_000)))
        port.playPath(playbackItem(ALBUM, gainDb = -6.020599913, trackId = 21, segment = segment(0, 10_000)))
        sink.setOutputStreamOffsetUs(0)
        sink.setOutputStreamOffsetUs(boundaryUs)

        sink.handleBuffer(pcm16(10_000), 0, 1)
        val first = probe.offers.last().samples.single()
        sink.handleBuffer(pcm16(10_000), boundaryUs, 1)
        val second = probe.offers.last().samples.single()

        assertEquals(5_000, first)
        assertEquals(20_000, second)
        port.release()
    }
}
