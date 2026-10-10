package io.github.marvinbaudach.reprise

import androidx.media3.common.C
import androidx.media3.common.Format
import androidx.media3.common.MediaItem
import androidx.media3.common.MimeTypes
import androidx.media3.common.Player
import androidx.media3.exoplayer.audio.AudioSink
import io.github.marvinbaudach.reprise.library.PlaybackKey
import io.github.marvinbaudach.reprise.library.PlaybackRequest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
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

    @Test
    fun play_20c_the_port_tells_the_sink_where_the_next_cue_track_continues_the_file() {
        val fake = CallbackPlayer(playbackState = Player.STATE_IDLE, playWhenReady = false)
        val sink = TrackGainAudioSink(probe.sink)
        val port = Media3PlaybackPort(fake.player, trackGainSink = sink) {}
        val file = "/music/album.flac"
        port.setNext(playbackItem(file, doubleDb, segment = AndroidPlaybackSegment(startMs = 3_000, endMs = 6_000)))
        port.playPath(playbackItem(file, halfDb, segment = AndroidPlaybackSegment(startMs = 0, endMs = 3_000)))
        val format = Format.Builder()
            .setSampleMimeType(MimeTypes.AUDIO_RAW)
            .setPcmEncoding(C.ENCODING_PCM_16BIT)
            .setSampleRate(1_000)
            .setChannelCount(1)
            .build()
        sink.configure(AudioSink.AudioSinkConfig.Builder(format).build())
        val offsetUs = 1_000_000_000_000L
        sink.setOutputStreamOffsetUs(offsetUs)

        // A buffer of 20 ms that starts 10 ms before the cut at 3 s into the file.
        sink.handleBuffer(pcm16(*IntArray(20) { 10_000 }), offsetUs + 2_990_000, 1)

        assertEquals(List(10) { 5_000 } + List(10) { 20_000 }, probe.offers.last().samples)
        port.release()
    }

    @Test
    fun play_20c_only_a_track_that_starts_where_the_current_one_ends_in_the_same_file_continues_it() {
        fun request(uri: String, startMs: Long, endMs: Long?) =
            PlaybackRequest(PlaybackKey(null, uri), AndroidPlaybackSegment(startMs = startMs, endMs = endMs))

        assertEquals(3_000_000L, continuationUs(request("a", 0, 3_000), request("a", 3_000, 6_000)))
        assertNull(continuationUs(request("a", 0, 3_000), request("a", 3_500, null)))
        assertNull(continuationUs(request("a", 0, 3_000), request("b", 3_000, null)))
        assertNull(continuationUs(request("a", 0, null), request("a", 3_000, null)))
        assertNull(continuationUs(PlaybackRequest(PlaybackKey(null, "a")), request("a", 0, null)))
        assertNull(continuationUs(null, request("a", 0, null)))
        assertNull(continuationUs(request("a", 0, 3_000), null))
    }
}
