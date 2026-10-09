package io.github.marvinbaudach.reprise

import androidx.media3.common.MediaItem
import androidx.media3.common.Player
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.reprise_android_ffi.AndroidPlaybackSegment

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class Media3PlaybackPortGainTest {
    private val halfDb = -6.020599913
    private val doubleDb = 6.020599913
    private val boundaryUs = 1_000_000L

    private val probe = RecordingAudioSink()

    private fun scaledAt(sink: TrackGainAudioSink, presentationTimeUs: Long): Int {
        sink.handleBuffer(pcm16(10_000), presentationTimeUs, 1)
        return probe.offers.last().samples.single()
    }

    @Test
    fun mtp_66_each_clip_of_a_cue_file_plays_at_its_own_gain_before_the_transition_event() {
        val fake = CallbackPlayer(playbackState = Player.STATE_IDLE, playWhenReady = false)
        val sink = TrackGainAudioSink(probe.sink)
        val port = Media3PlaybackPort(fake.player, trackGainSink = sink) {}
        val cueFile = "/music/album.flac"
        val sameOffsetUs = 1_000_000_000_000L
        val firstClip = AndroidPlaybackSegment(startMs = 0, endMs = 3_000)
        val secondClip = AndroidPlaybackSegment(startMs = 3_000, endMs = 6_000)
        port.setNext(playbackItem(cueFile, doubleDb, segment = secondClip))
        port.playPath(playbackItem(cueFile, halfDb, segment = firstClip))
        sink.setOutputStreamOffsetUs(sameOffsetUs)
        assertEquals(5_000, scaledAt(sink, sameOffsetUs))

        sink.setOutputStreamOffsetUs(sameOffsetUs)
        assertEquals(20_000, scaledAt(sink, boundaryUs))

        fake.listener.onMediaItemTransition(
            MediaItem.fromUri(cueFile),
            Player.MEDIA_ITEM_TRANSITION_REASON_AUTO,
        )
        assertEquals(20_000, scaledAt(sink, boundaryUs + 1))
        port.release()
    }

    @Test
    fun aLiveGainChangeReachesTheCurrentAndThePreFedTrackWithoutRequeueing() {
        val fake = CallbackPlayer(playbackState = Player.STATE_IDLE, playWhenReady = false)
        val sink = TrackGainAudioSink(probe.sink)
        val port = Media3PlaybackPort(fake.player, trackGainSink = sink) {}
        port.setNext(playbackItem("/music/b.flac", 0.0))
        port.playPath(playbackItem("/music/a.flac", 0.0))
        sink.setOutputStreamOffsetUs(0)
        val queued = fake.mediaItems.toList()

        port.setGains(halfDb, doubleDb)

        assertEquals(5_000, scaledAt(sink, 100))
        sink.setOutputStreamOffsetUs(boundaryUs)
        assertEquals(20_000, scaledAt(sink, boundaryUs))
        assertEquals(queued, fake.mediaItems)

        // The next track is dropped from the player: a later call with no next
        // gain must not bring its gain back.
        port.setNext(null)
        port.setGains(halfDb, null)
        assertEquals(5_000, scaledAt(sink, boundaryUs))
        port.release()
    }
}
