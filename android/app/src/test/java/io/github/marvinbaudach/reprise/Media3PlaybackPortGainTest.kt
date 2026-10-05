package io.github.marvinbaudach.reprise

import androidx.media3.common.MediaItem
import androidx.media3.common.Player
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

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
    fun theAutomaticTransitionMakesTheNextTracksGainTheCurrentOne() {
        val fake = CallbackPlayer(playbackState = Player.STATE_IDLE, playWhenReady = false)
        val sink = TrackGainAudioSink(probe.sink)
        val port = Media3PlaybackPort(fake.player, sink) {}
        port.setNext("/music/b.flac", doubleDb)
        port.playPath("/music/a.flac", halfDb)
        sink.setOutputStreamOffsetUs(0)
        sink.setOutputStreamOffsetUs(boundaryUs)
        assertEquals(20_000, scaledAt(sink, boundaryUs))

        fake.listener.onMediaItemTransition(
            MediaItem.fromUri("/music/b.flac"),
            Player.MEDIA_ITEM_TRANSITION_REASON_AUTO,
        )
        port.setNext("/music/c.flac", halfDb)
        sink.setOutputStreamOffsetUs(boundaryUs)
        sink.setOutputStreamOffsetUs(2 * boundaryUs)
        sink.flush()

        // A seek back inside the track now playing keeps that track's gain.
        assertEquals(20_000, scaledAt(sink, boundaryUs + 1))
        assertEquals(5_000, scaledAt(sink, 2 * boundaryUs))
        port.release()
    }

    @Test
    fun aLiveGainChangeReachesTheCurrentAndThePreFedTrackWithoutRequeueing() {
        val fake = CallbackPlayer(playbackState = Player.STATE_IDLE, playWhenReady = false)
        val sink = TrackGainAudioSink(probe.sink)
        val port = Media3PlaybackPort(fake.player, sink) {}
        port.setNext("/music/b.flac", 0.0)
        port.playPath("/music/a.flac", 0.0)
        sink.setOutputStreamOffsetUs(0)
        sink.setOutputStreamOffsetUs(boundaryUs)
        val queued = fake.mediaItems.toList()

        port.setGains(halfDb, doubleDb)

        assertEquals(5_000, scaledAt(sink, 100))
        assertEquals(20_000, scaledAt(sink, boundaryUs))
        assertEquals(queued, fake.mediaItems)

        // The next track is dropped from the player: a later call with no next
        // gain must not bring its gain back.
        port.setNext(null, 0.0)
        port.setGains(halfDb, null)
        assertEquals(5_000, scaledAt(sink, boundaryUs))
        port.release()
    }
}
