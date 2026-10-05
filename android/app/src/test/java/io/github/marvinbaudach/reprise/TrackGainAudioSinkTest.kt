package io.github.marvinbaudach.reprise

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotSame
import org.junit.Assert.assertSame
import org.junit.Test

private const val HALF_DB = -6.020599913
private const val DOUBLE_DB = 6.020599913
private const val BOUNDARY_US = 1_000_000L

/** A gain sink in front of a recording delegate: what it forwards is what plays. */
private class Rig {
    val probe = RecordingAudioSink()
    val sink = TrackGainAudioSink(probe.sink)

    /** Offers [samples] at [presentationTimeUs] and returns what the output stage got. */
    fun offer(presentationTimeUs: Long, vararg samples: Int): List<Int> {
        sink.handleBuffer(pcm16(*samples), presentationTimeUs, 1)
        return probe.offers.last().samples
    }

    fun scaledAt(presentationTimeUs: Long): Int = offer(presentationTimeUs, 10_000).single()

    fun startTwoTracks(currentGainDb: Double, nextGainDb: Double?) {
        sink.startPlaylist(currentGainDb, nextGainDb)
        sink.setOutputStreamOffsetUs(0)
        nextGainDb?.let { sink.setOutputStreamOffsetUs(BOUNDARY_US) }
    }
}

class TrackGainAudioSinkTest {
    @Test
    fun play_19c_gain_switches_when_the_first_buffer_reaches_the_queued_stream_offset() {
        val rig = Rig()
        rig.startTwoTracks(HALF_DB, DOUBLE_DB)

        assertEquals(listOf(5_000, -5_000), rig.offer(0, 10_000, -10_000))
        assertEquals(5_000, rig.scaledAt(BOUNDARY_US - 1))
        assertEquals(20_000, rig.scaledAt(BOUNDARY_US))
    }

    @Test
    fun positiveGainSaturatesSignedPcm16InsteadOfWrapping() {
        val rig = Rig()
        rig.startTwoTracks(12.0, null)

        assertEquals(
            listOf(Short.MAX_VALUE.toInt(), Short.MIN_VALUE.toInt()),
            rig.offer(0, 20_000, -20_000),
        )
    }

    @Test
    fun aRepeatedOffsetForTheSameStreamKeepsTheQueuedNextGain() {
        val rig = Rig()
        rig.sink.startPlaylist(0.0, DOUBLE_DB)
        rig.sink.setOutputStreamOffsetUs(0)
        rig.sink.setOutputStreamOffsetUs(0)

        rig.sink.setOutputStreamOffsetUs(BOUNDARY_US)

        assertEquals(20_000, rig.scaledAt(BOUNDARY_US))
    }

    @Test
    fun aGainThatIsNotFiniteLeavesTheSamplesUntouched() {
        listOf(Double.NaN, Double.POSITIVE_INFINITY, Double.NEGATIVE_INFINITY).forEach { hostile ->
            val rig = Rig()
            rig.startTwoTracks(hostile, null)

            assertEquals("gain $hostile", listOf(10_000, -10_000), rig.offer(0, 10_000, -10_000))
        }
    }

    @Test
    fun aGainOutsideTheSafeRangeIsClampedBeforeTheAudioThreadUsesIt() {
        val loud = Rig()
        loud.startTwoTracks(400.0, null)
        // +12 dB is a factor of about 3.98, not 10^20.
        assertEquals(398, loud.offer(0, 100).single())

        val muted = Rig()
        muted.startTwoTracks(-400.0, null)
        // -24 dB is a factor of about 0.063, not silence by underflow.
        assertEquals(631, muted.offer(0, 10_000).single())
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

    @Test
    fun play_19c_a_next_track_replaced_after_its_offset_was_announced_plays_with_the_new_gain() {
        val rig = Rig()
        rig.startTwoTracks(0.0, DOUBLE_DB)

        // The next track is replaced; Media3 re-reads it and announces the
        // same offset again, which must not be mistaken for the old stream.
        rig.sink.setNextGain(HALF_DB)
        rig.sink.setOutputStreamOffsetUs(BOUNDARY_US)

        assertEquals(10_000, rig.scaledAt(BOUNDARY_US - 1))
        assertEquals(5_000, rig.scaledAt(BOUNDARY_US))
    }

    @Test
    fun play_19c_a_backward_seek_across_the_boundary_plays_the_earlier_track_with_its_own_gain() {
        val rig = Rig()
        rig.startTwoTracks(HALF_DB, DOUBLE_DB)
        assertEquals(20_000, rig.scaledAt(BOUNDARY_US))

        // The seek flushes the sink and Media3 announces the first stream again.
        rig.sink.flush()
        rig.sink.setOutputStreamOffsetUs(0)
        rig.sink.setOutputStreamOffsetUs(BOUNDARY_US)

        assertEquals(5_000, rig.scaledAt(500_000))
        assertEquals(20_000, rig.scaledAt(BOUNDARY_US))
    }

    @Test
    fun aFlushWithoutANewAnnouncementDoesNotLeaveTheNextTracksGainBehind() {
        val rig = Rig()
        rig.startTwoTracks(HALF_DB, DOUBLE_DB)
        assertEquals(20_000, rig.scaledAt(BOUNDARY_US))

        rig.sink.flush()

        assertEquals(5_000, rig.scaledAt(500_000))
    }

    @Test
    fun afterTheAutomaticTransitionTheNextTrackIsTheCurrentOne() {
        val rig = Rig()
        rig.startTwoTracks(HALF_DB, DOUBLE_DB)
        assertEquals(20_000, rig.scaledAt(BOUNDARY_US))

        rig.sink.advanceToNext()
        rig.sink.setNextGain(HALF_DB)
        rig.sink.setOutputStreamOffsetUs(BOUNDARY_US)
        rig.sink.setOutputStreamOffsetUs(2 * BOUNDARY_US)

        // A seek inside the track that is now current keeps its gain, and the
        // newly fed track takes over at its own offset.
        rig.sink.flush()
        assertEquals(20_000, rig.scaledAt(BOUNDARY_US + 1))
        assertEquals(5_000, rig.scaledAt(2 * BOUNDARY_US))
    }

    @Test
    fun aLiveGainChangeReachesTheCurrentAndTheNextTrack() {
        val rig = Rig()
        rig.startTwoTracks(0.0, 0.0)
        assertEquals(10_000, rig.scaledAt(100))

        rig.sink.setGains(HALF_DB, DOUBLE_DB)

        assertEquals(5_000, rig.scaledAt(100))
        assertEquals(20_000, rig.scaledAt(BOUNDARY_US))
    }

    @Test
    fun aNextTrackThatWasRemovedStopsApplyingItsGain() {
        val rig = Rig()
        rig.startTwoTracks(0.0, DOUBLE_DB)

        rig.sink.setNextGain(null)

        assertEquals(10_000, rig.scaledAt(BOUNDARY_US))
    }

    @Test
    fun play_19c_a_read_only_input_buffer_is_scaled_without_being_written_into() {
        val rig = Rig()
        rig.startTwoTracks(HALF_DB, null)
        // MediaCodec output buffers can be read-only, and a read-only view is
        // big-endian whatever the data is; neither may matter to the sink.
        val input = pcm16(10_000, -10_000).asReadOnlyBuffer()

        val consumed = rig.sink.handleBuffer(input, 0, 1)

        assertEquals(true, consumed)
        assertEquals(listOf(5_000, -5_000), rig.probe.offers.single().samples)
        assertNotSame(input, rig.probe.offers.single().buffer)
        assertEquals(0, input.remaining())
        input.rewind()
        assertEquals(listOf(10_000, -10_000), remainingSamples(input))
    }

    @Test
    fun play_19c_a_partially_consumed_buffer_is_retried_from_the_same_scaled_copy() {
        val rig = Rig()
        rig.startTwoTracks(HALF_DB, DOUBLE_DB)
        rig.probe.consumeBytesPerCall = Short.SIZE_BYTES
        val input = pcm16(10_000, 20_000, -10_000)
        val start = input.position()

        assertEquals(false, rig.sink.handleBuffer(input, 0, 1))
        assertEquals(start + Short.SIZE_BYTES, input.position())

        // The gain moves between the retries; the buffer in flight keeps the
        // gain it was scaled with, or the second half would play at another one.
        rig.sink.setGains(DOUBLE_DB, null)
        assertEquals(false, rig.sink.handleBuffer(input, 0, 1))
        assertEquals(start + 2 * Short.SIZE_BYTES, input.position())
        assertEquals(true, rig.sink.handleBuffer(input, 0, 1))
        assertEquals(input.limit(), input.position())

        val offers = rig.probe.offers
        assertEquals(3, offers.size)
        assertSame(offers[0].buffer, offers[1].buffer)
        assertSame(offers[0].buffer, offers[2].buffer)
        assertEquals(listOf(5_000, 10_000, -5_000), offers[0].samples)
        assertEquals(listOf(10_000, -5_000), offers[1].samples)
        assertEquals(listOf(-5_000), offers[2].samples)
    }

    @Test
    fun theScaledCopyIsReusedAcrossBuffersAndGrowsOnlyWhenNeeded() {
        val rig = Rig()
        rig.startTwoTracks(HALF_DB, null)

        rig.offer(0, 10_000, 10_000)
        rig.offer(1, 10_000)
        val small = rig.probe.offers.map { it.buffer }
        assertSame(small[0], small[1])

        rig.offer(2, *IntArray(100_000) { 1_000 })
        val grown = rig.probe.offers.last().buffer
        assertEquals(100_000, rig.probe.offers.last().samples.size)
        assertEquals(500, rig.probe.offers.last().samples.first())

        rig.offer(3, 10_000)
        assertSame(grown, rig.probe.offers.last().buffer)
    }

    @Test
    fun aFlushEndsTheRetryAndTheNextOfferIsScaledAnew() {
        val rig = Rig()
        rig.startTwoTracks(HALF_DB, null)
        rig.probe.consumeBytesPerCall = Short.SIZE_BYTES
        val input = pcm16(10_000, 20_000)
        rig.sink.handleBuffer(input, 0, 1)

        rig.sink.flush()
        rig.sink.setGains(DOUBLE_DB, null)
        rig.probe.consumeBytesPerCall = Int.MAX_VALUE
        rig.sink.handleBuffer(input, 0, 1)

        // What was left of the input is scaled with the gain now in force.
        assertEquals(listOf(40_000.coerceAtMost(Short.MAX_VALUE.toInt())), rig.probe.offers.last().samples)
    }

    @Test
    fun aUnityGainForwardsTheInputBufferItselfUntouched() {
        val rig = Rig()
        rig.startTwoTracks(0.0, null)
        val input = pcm16(10_000, -10_000)

        assertEquals(true, rig.sink.handleBuffer(input, 0, 1))

        assertSame(input, rig.probe.offers.single().buffer)
        assertEquals(listOf(10_000, -10_000), rig.probe.offers.single().samples)
    }

    @Test
    fun aBufferForwardedAtUnityStaysUntouchedAcrossItsRetries() {
        val rig = Rig()
        rig.startTwoTracks(0.0, null)
        rig.probe.consumeBytesPerCall = Short.SIZE_BYTES
        val input = pcm16(10_000, 20_000)

        assertEquals(false, rig.sink.handleBuffer(input, 0, 1))
        rig.sink.setGains(HALF_DB, null)
        assertEquals(true, rig.sink.handleBuffer(input, 0, 1))

        rig.probe.offers.forEach { assertSame(input, it.buffer) }
        assertEquals(listOf(20_000), rig.probe.offers.last().samples)
    }

    @Test
    fun aGainAwayFromUnityStillScalesIntoTheCopy() {
        val rig = Rig()
        rig.startTwoTracks(0.1, null)
        val input = pcm16(10_000)

        rig.sink.handleBuffer(input, 0, 1)

        assertNotSame(input, rig.probe.offers.single().buffer)
        assertEquals(10_116, rig.probe.offers.single().samples.single())
    }

    @Test
    fun play_19c_a_current_track_announced_again_at_a_smaller_offset_keeps_its_own_gain() {
        val rig = Rig()
        rig.startTwoTracks(0.0, DOUBLE_DB)
        rig.sink.advanceToNext()
        rig.sink.setNextGain(HALF_DB)
        rig.sink.setOutputStreamOffsetUs(3 * BOUNDARY_US)
        assertEquals(5_000, rig.scaledAt(3 * BOUNDARY_US))

        // A seek resets the playing track's offset to a smaller value than it
        // had; the offsets of the tracks after it are announced anew.
        rig.sink.flush()
        rig.sink.setOutputStreamOffsetUs(0)
        assertEquals(20_000, rig.scaledAt(500_000))
        rig.sink.setOutputStreamOffsetUs(2 * BOUNDARY_US)

        assertEquals(20_000, rig.scaledAt(2 * BOUNDARY_US - 1))
        assertEquals(5_000, rig.scaledAt(2 * BOUNDARY_US))
    }

    @Test
    fun play_19c_a_smaller_offset_for_the_current_track_does_not_bind_the_unannounced_next() {
        val rig = Rig()
        rig.startTwoTracks(0.0, DOUBLE_DB)
        rig.sink.advanceToNext()
        rig.sink.setNextGain(HALF_DB)

        rig.sink.flush()
        rig.sink.setOutputStreamOffsetUs(0)

        assertEquals(20_000, rig.scaledAt(500_000))
        assertEquals(20_000, rig.scaledAt(5 * BOUNDARY_US))
    }
}
