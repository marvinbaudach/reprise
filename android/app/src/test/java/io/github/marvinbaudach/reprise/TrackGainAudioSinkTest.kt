package io.github.marvinbaudach.reprise

import androidx.media3.exoplayer.audio.AudioSink
import java.lang.reflect.Proxy
import java.nio.ByteBuffer
import java.nio.ByteOrder
import org.junit.Assert.assertEquals
import org.junit.Test

internal fun inertAudioSink(): AudioSink = Proxy.newProxyInstance(
    AudioSink::class.java.classLoader,
    arrayOf(AudioSink::class.java),
) { _, method, _ ->
    when (method.returnType) {
        java.lang.Boolean.TYPE -> true
        java.lang.Integer.TYPE -> 0
        java.lang.Long.TYPE -> 0L
        java.lang.Float.TYPE -> 0f
        else -> null
    }
} as AudioSink

class TrackGainAudioSinkTest {
    private fun delegate(): AudioSink = inertAudioSink()

    private fun pcm16(vararg samples: Int): ByteBuffer = ByteBuffer
        .allocate(samples.size * Short.SIZE_BYTES)
        .order(ByteOrder.LITTLE_ENDIAN)
        .apply {
            samples.forEach { putShort(it.toShort()) }
            flip()
        }

    @Test
    fun play_19c_gain_switches_when_the_first_buffer_reaches_the_queued_stream_offset() {
        val sink = TrackGainAudioSink(delegate())
        sink.startPlaylist(-6.020599913, 6.020599913)
        sink.setOutputStreamOffsetUs(0)
        val first = pcm16(10_000, -10_000)

        sink.handleBuffer(first, 0, 1)

        assertEquals(5_000, first.getShort(0).toInt())
        assertEquals(-5_000, first.getShort(2).toInt())

        sink.setOutputStreamOffsetUs(1_000_000)
        val beforeBoundary = pcm16(10_000)
        sink.handleBuffer(beforeBoundary, 999_999, 1)
        assertEquals(5_000, beforeBoundary.getShort(0).toInt())

        val atBoundary = pcm16(10_000)
        sink.handleBuffer(atBoundary, 1_000_000, 1)
        assertEquals(20_000, atBoundary.getShort(0).toInt())
    }

    @Test
    fun positiveGainSaturatesSignedPcm16InsteadOfWrapping() {
        val sink = TrackGainAudioSink(delegate())
        sink.startPlaylist(12.0, null)
        sink.setOutputStreamOffsetUs(0)
        val buffer = pcm16(20_000, -20_000)

        sink.handleBuffer(buffer, 0, 1)

        assertEquals(Short.MAX_VALUE.toInt(), buffer.getShort(0).toInt())
        assertEquals(Short.MIN_VALUE.toInt(), buffer.getShort(2).toInt())
    }

    @Test
    fun aRepeatedOffsetForTheSameStreamKeepsTheQueuedNextGain() {
        val sink = TrackGainAudioSink(delegate())
        sink.startPlaylist(0.0, 6.020599913)
        sink.setOutputStreamOffsetUs(0)
        sink.setOutputStreamOffsetUs(0)

        sink.setOutputStreamOffsetUs(1_000_000)
        val next = pcm16(10_000)
        sink.handleBuffer(next, 1_000_000, 1)

        assertEquals(20_000, next.getShort(0).toInt())
    }

    @Test
    fun aGainThatIsNotFiniteLeavesTheSamplesUntouched() {
        listOf(Double.NaN, Double.POSITIVE_INFINITY, Double.NEGATIVE_INFINITY).forEach { hostile ->
            val sink = TrackGainAudioSink(delegate())
            sink.startPlaylist(hostile, null)
            sink.setOutputStreamOffsetUs(0)
            val buffer = pcm16(10_000, -10_000)

            sink.handleBuffer(buffer, 0, 1)

            assertEquals("gain $hostile", 10_000, buffer.getShort(0).toInt())
            assertEquals("gain $hostile", -10_000, buffer.getShort(2).toInt())
        }
    }

    @Test
    fun aGainOutsideTheSafeRangeIsClampedBeforeTheAudioThreadUsesIt() {
        val loud = TrackGainAudioSink(delegate())
        loud.startPlaylist(400.0, null)
        loud.setOutputStreamOffsetUs(0)
        val quiet = pcm16(100)
        loud.handleBuffer(quiet, 0, 1)
        // +12 dB is a factor of about 3.98, not 10^20.
        assertEquals(398, quiet.getShort(0).toInt())

        val muted = TrackGainAudioSink(delegate())
        muted.startPlaylist(-400.0, null)
        muted.setOutputStreamOffsetUs(0)
        val loudInput = pcm16(10_000)
        muted.handleBuffer(loudInput, 0, 1)
        // -24 dB is a factor of about 0.063, not silence by underflow.
        assertEquals(631, loudInput.getShort(0).toInt())
    }

    @Test
    fun linearGainIsAlwaysFiniteAndBounded() {
        assertEquals(1.0, TrackGainAudioSink.linearGain(Double.NaN), 0.0)
        assertEquals(1.0, TrackGainAudioSink.linearGain(Double.POSITIVE_INFINITY), 0.0)
        assertEquals(1.0, TrackGainAudioSink.linearGain(Double.NEGATIVE_INFINITY), 0.0)
        assertEquals(3.981, TrackGainAudioSink.linearGain(1_000.0), 0.001)
        assertEquals(0.0631, TrackGainAudioSink.linearGain(-1_000.0), 0.0001)
        assertEquals(2.0, TrackGainAudioSink.linearGain(6.0206), 0.001)
    }

    /** The first sample of a one-sample buffer at [presentationTimeUs], scaled. */
    private fun scaledAt(sink: TrackGainAudioSink, presentationTimeUs: Long): Int {
        val buffer = pcm16(10_000)
        sink.handleBuffer(buffer, presentationTimeUs, 1)
        return buffer.getShort(0).toInt()
    }

    private companion object {
        const val HALF_DB = -6.020599913
        const val DOUBLE_DB = 6.020599913
        const val BOUNDARY_US = 1_000_000L
    }

    @Test
    fun aNextTrackReplacedAfterItsOffsetWasAnnouncedPlaysWithTheNewGain() {
        val sink = TrackGainAudioSink(delegate())
        sink.startPlaylist(0.0, DOUBLE_DB)
        sink.setOutputStreamOffsetUs(0)
        sink.setOutputStreamOffsetUs(BOUNDARY_US)

        // The next track is replaced; Media3 re-reads it and announces the
        // same offset again, which must not be mistaken for the old stream.
        sink.setNextGain(HALF_DB)
        sink.setOutputStreamOffsetUs(BOUNDARY_US)

        assertEquals(10_000, scaledAt(sink, BOUNDARY_US - 1))
        assertEquals(5_000, scaledAt(sink, BOUNDARY_US))
    }

    @Test
    fun aBackwardSeekAcrossTheBoundaryPlaysTheEarlierTrackWithItsOwnGain() {
        val sink = TrackGainAudioSink(delegate())
        sink.startPlaylist(HALF_DB, DOUBLE_DB)
        sink.setOutputStreamOffsetUs(0)
        sink.setOutputStreamOffsetUs(BOUNDARY_US)
        assertEquals(20_000, scaledAt(sink, BOUNDARY_US))

        // The seek flushes the sink and Media3 announces the first stream again.
        sink.flush()
        sink.setOutputStreamOffsetUs(0)
        sink.setOutputStreamOffsetUs(BOUNDARY_US)

        assertEquals(5_000, scaledAt(sink, 500_000))
        assertEquals(20_000, scaledAt(sink, BOUNDARY_US))
    }

    @Test
    fun aFlushWithoutANewAnnouncementDoesNotLeaveTheNextTracksGainBehind() {
        val sink = TrackGainAudioSink(delegate())
        sink.startPlaylist(HALF_DB, DOUBLE_DB)
        sink.setOutputStreamOffsetUs(0)
        sink.setOutputStreamOffsetUs(BOUNDARY_US)
        assertEquals(20_000, scaledAt(sink, BOUNDARY_US))

        sink.flush()

        assertEquals(5_000, scaledAt(sink, 500_000))
    }

    @Test
    fun afterTheAutomaticTransitionTheNextTrackIsTheCurrentOne() {
        val sink = TrackGainAudioSink(delegate())
        sink.startPlaylist(HALF_DB, DOUBLE_DB)
        sink.setOutputStreamOffsetUs(0)
        sink.setOutputStreamOffsetUs(BOUNDARY_US)
        assertEquals(20_000, scaledAt(sink, BOUNDARY_US))

        sink.advanceToNext()
        sink.setNextGain(HALF_DB)
        sink.setOutputStreamOffsetUs(BOUNDARY_US)
        sink.setOutputStreamOffsetUs(2 * BOUNDARY_US)

        // A seek inside the track that is now current keeps its gain, and the
        // newly fed track takes over at its own offset.
        sink.flush()
        assertEquals(20_000, scaledAt(sink, BOUNDARY_US + 1))
        assertEquals(5_000, scaledAt(sink, 2 * BOUNDARY_US))
    }

    @Test
    fun aLiveGainChangeReachesTheCurrentAndTheNextTrack() {
        val sink = TrackGainAudioSink(delegate())
        sink.startPlaylist(0.0, 0.0)
        sink.setOutputStreamOffsetUs(0)
        sink.setOutputStreamOffsetUs(BOUNDARY_US)
        assertEquals(10_000, scaledAt(sink, 100))

        sink.setGains(HALF_DB, DOUBLE_DB)

        assertEquals(5_000, scaledAt(sink, 100))
        assertEquals(20_000, scaledAt(sink, BOUNDARY_US))
    }

    @Test
    fun aNextTrackThatWasRemovedStopsApplyingItsGain() {
        val sink = TrackGainAudioSink(delegate())
        sink.startPlaylist(0.0, DOUBLE_DB)
        sink.setOutputStreamOffsetUs(0)
        sink.setOutputStreamOffsetUs(BOUNDARY_US)

        sink.setNextGain(null)

        assertEquals(10_000, scaledAt(sink, BOUNDARY_US))
    }
}
