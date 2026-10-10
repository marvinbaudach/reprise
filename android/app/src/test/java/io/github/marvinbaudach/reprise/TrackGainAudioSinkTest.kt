package io.github.marvinbaudach.reprise

import androidx.media3.common.C
import androidx.media3.common.Format
import androidx.media3.common.MimeTypes
import androidx.media3.exoplayer.audio.AudioSink
import androidx.media3.exoplayer.source.MediaSource
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotSame
import org.junit.Assert.assertSame
import org.junit.Test

private const val HALF_DB = -6.020599913
private const val DOUBLE_DB = 6.020599913
private const val BOUNDARY_US = 1_000_000L
private const val OFFSET_US = 1_000_000_000_000L
private const val CUT_US = 3_000_000L
private const val FRAME_US = 1_000L

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
    }

    /**
     * Two tracks cut from one file, the second starting at [CUT_US] into it, on a
     * mono 1 kHz stream (one frame per millisecond) whose timestamps Media3 shifted
     * by [OFFSET_US].
     */
    fun startTwoCueTracks(configured: Boolean = true) {
        sink.startPlaylist(HALF_DB, DOUBLE_DB, CUT_US)
        if (configured) {
            val format = Format.Builder()
                .setSampleMimeType(MimeTypes.AUDIO_RAW)
                .setPcmEncoding(C.ENCODING_PCM_16BIT)
                .setSampleRate(1_000)
                .setChannelCount(1)
                .build()
            sink.configure(AudioSink.AudioSinkConfig.Builder(format).build())
        }
        sink.setOutputStreamOffsetUs(OFFSET_US)
    }

    /** Configures the sink for a stream of the period [periodUid], or of none. */
    fun configureFor(periodUid: Any?) {
        val format = Format.Builder()
            .setSampleMimeType(MimeTypes.AUDIO_RAW)
            .setPcmEncoding(C.ENCODING_PCM_16BIT)
            .setSampleRate(1_000)
            .setChannelCount(1)
            .build()
        val config = AudioSink.AudioSinkConfig.Builder(format)
            .setMediaPeriodId(periodUid?.let { MediaSource.MediaPeriodId(it) })
            .build()
        sink.configure(config)
    }

    /** Offers [frames] frames of 10 000 starting [fileUs] into the file. */
    fun offerFromFile(fileUs: Long, frames: Int): List<Int> =
        offer(OFFSET_US + fileUs, *IntArray(frames) { 10_000 })
}

class TrackGainAudioSinkTest {
    @Test
    fun play_20c_gain_switches_at_the_first_buffer_after_the_stream_announcement() {
        val rig = Rig()
        rig.startTwoTracks(HALF_DB, DOUBLE_DB)

        assertEquals(listOf(5_000, -5_000), rig.offer(0, 10_000, -10_000))
        assertEquals(5_000, rig.scaledAt(BOUNDARY_US - 1))
        rig.sink.setOutputStreamOffsetUs(BOUNDARY_US)
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
    fun play_20c_an_announcement_before_any_buffer_is_not_a_stream_change() {
        val rig = Rig()
        rig.sink.startPlaylist(0.0, DOUBLE_DB)
        rig.sink.setOutputStreamOffsetUs(0)
        rig.sink.setOutputStreamOffsetUs(0)
        assertEquals(10_000, rig.scaledAt(0))

        rig.sink.setOutputStreamOffsetUs(BOUNDARY_US)
        assertEquals(20_000, rig.scaledAt(BOUNDARY_US))

        rig.sink.flush()
        rig.sink.setOutputStreamOffsetUs(2 * BOUNDARY_US)
        assertEquals(10_000, rig.scaledAt(2 * BOUNDARY_US))
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
    fun play_20c_a_next_track_replaced_after_its_offset_was_announced_plays_with_the_new_gain() {
        val rig = Rig()
        rig.startTwoTracks(0.0, DOUBLE_DB)
        assertEquals(10_000, rig.scaledAt(BOUNDARY_US - 1))
        rig.sink.setOutputStreamOffsetUs(BOUNDARY_US)

        // The next track is replaced after Media3 announced it. The active
        // item keeps its identity and takes the replacement gain in place.
        rig.sink.setNextGain(HALF_DB)

        assertEquals(5_000, rig.scaledAt(BOUNDARY_US))
    }

    @Test
    fun play_20c_a_backward_seek_across_the_boundary_plays_the_earlier_track_with_its_own_gain() {
        val rig = Rig()
        rig.startTwoTracks(HALF_DB, DOUBLE_DB)
        assertEquals(5_000, rig.scaledAt(BOUNDARY_US - 1))
        rig.sink.setOutputStreamOffsetUs(BOUNDARY_US)
        assertEquals(20_000, rig.scaledAt(BOUNDARY_US))

        // The seek flushes the sink and Media3 announces the first stream again.
        rig.sink.flush()
        rig.sink.setOutputStreamOffsetUs(0)
        assertEquals(5_000, rig.scaledAt(500_000))
        rig.sink.setOutputStreamOffsetUs(BOUNDARY_US)
        assertEquals(20_000, rig.scaledAt(BOUNDARY_US))
    }

    @Test
    fun aFlushWithoutANewAnnouncementDoesNotLeaveTheNextTracksGainBehind() {
        val rig = Rig()
        rig.startTwoTracks(HALF_DB, DOUBLE_DB)
        rig.scaledAt(BOUNDARY_US - 1)
        rig.sink.setOutputStreamOffsetUs(BOUNDARY_US)
        assertEquals(20_000, rig.scaledAt(BOUNDARY_US))

        rig.sink.flush()

        assertEquals(5_000, rig.scaledAt(500_000))
    }

    @Test
    fun afterTheAutomaticTransitionTheNextTrackIsTheCurrentOne() {
        val rig = Rig()
        rig.startTwoTracks(HALF_DB, DOUBLE_DB)
        rig.scaledAt(BOUNDARY_US - 1)
        rig.sink.setOutputStreamOffsetUs(BOUNDARY_US)
        assertEquals(20_000, rig.scaledAt(BOUNDARY_US))

        rig.sink.advanceToNext()
        rig.sink.setNextGain(HALF_DB)

        // A seek inside the track that is now current keeps its gain. Media3
        // announces the newly fed stream only after the current stream writes.
        rig.sink.flush()
        assertEquals(20_000, rig.scaledAt(BOUNDARY_US + 1))
        rig.sink.setOutputStreamOffsetUs(2 * BOUNDARY_US)
        assertEquals(5_000, rig.scaledAt(2 * BOUNDARY_US))
    }

    @Test
    fun aLiveGainChangeReachesTheCurrentAndTheNextTrack() {
        val rig = Rig()
        rig.startTwoTracks(0.0, 0.0)
        assertEquals(10_000, rig.scaledAt(100))

        rig.sink.setGains(HALF_DB, DOUBLE_DB)

        assertEquals(5_000, rig.scaledAt(100))
        rig.sink.setOutputStreamOffsetUs(BOUNDARY_US)
        assertEquals(20_000, rig.scaledAt(BOUNDARY_US))
    }

    @Test
    fun aNextTrackThatWasRemovedStopsApplyingItsGain() {
        val rig = Rig()
        rig.startTwoTracks(0.0, DOUBLE_DB)

        rig.sink.setNextGain(null)
        rig.scaledAt(BOUNDARY_US - 1)
        rig.sink.setOutputStreamOffsetUs(BOUNDARY_US)

        assertEquals(10_000, rig.scaledAt(BOUNDARY_US))
    }

    @Test
    fun play_20c_a_read_only_input_buffer_is_scaled_without_being_written_into() {
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
    fun play_20c_a_partially_consumed_buffer_is_retried_from_the_same_scaled_copy() {
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
    fun play_20c_two_clips_of_one_file_announced_at_the_same_offset_each_play_with_their_own_gain() {
        val rig = Rig()
        val sameOffsetUs = 1_000_000_000_000L
        rig.sink.startPlaylist(HALF_DB, DOUBLE_DB)
        rig.sink.setOutputStreamOffsetUs(sameOffsetUs)

        assertEquals(5_000, rig.scaledAt(sameOffsetUs))
        rig.sink.setOutputStreamOffsetUs(sameOffsetUs)
        assertEquals(20_000, rig.scaledAt(sameOffsetUs))

        rig.sink.advanceToNext()
        rig.sink.setNextGain(HALF_DB)
        rig.sink.setOutputStreamOffsetUs(sameOffsetUs)
        assertEquals(5_000, rig.scaledAt(sameOffsetUs))
    }

    @Test
    fun play_20c_a_stream_announced_at_a_smaller_offset_than_its_predecessor_gets_its_own_gain() {
        val rig = Rig()
        val baseOffsetUs = 1_000_000_000_000L
        rig.sink.startPlaylist(HALF_DB, DOUBLE_DB)
        rig.sink.setOutputStreamOffsetUs(baseOffsetUs)
        assertEquals(5_000, rig.scaledAt(baseOffsetUs))

        rig.sink.setOutputStreamOffsetUs(baseOffsetUs + 6_000_000L)
        assertEquals(20_000, rig.scaledAt(baseOffsetUs + 9_000_000L))
        rig.sink.advanceToNext()
        rig.sink.setNextGain(HALF_DB)

        rig.sink.setOutputStreamOffsetUs(baseOffsetUs + 3_000_000L)
        assertEquals(5_000, rig.scaledAt(baseOffsetUs + 9_000_000L))
    }

    @Test
    fun play_20c_the_next_tracks_gain_is_applied_before_the_transition_event_fires() {
        val rig = Rig()
        val sameOffsetUs = 1_000_000_000_000L
        rig.sink.startPlaylist(HALF_DB, DOUBLE_DB)
        rig.sink.setOutputStreamOffsetUs(sameOffsetUs)
        assertEquals(5_000, rig.scaledAt(sameOffsetUs))

        rig.sink.setOutputStreamOffsetUs(sameOffsetUs)
        assertEquals(20_000, rig.scaledAt(sameOffsetUs))

        rig.sink.advanceToNext()
        assertEquals(20_000, rig.scaledAt(sameOffsetUs + 1))
    }

    @Test
    fun play_20c_a_buffer_holding_the_cut_switches_gain_at_the_cut_sample() {
        val rig = Rig()
        rig.startTwoCueTracks()

        // 100 frames from 2.95 s: the cut at 3.0 s is the 51st frame.
        val out = rig.offerFromFile(CUT_US - 50 * FRAME_US, 100)

        assertEquals(List(50) { 5_000 } + List(50) { 20_000 }, out)
    }

    @Test
    fun play_20c_a_buffer_that_starts_at_the_cut_has_the_next_tracks_gain_throughout() {
        val rig = Rig()
        rig.startTwoCueTracks()

        assertEquals(List(10) { 20_000 }, rig.offerFromFile(CUT_US, 10))
    }

    @Test
    fun play_20c_a_buffer_that_ends_at_the_cut_keeps_the_current_tracks_gain() {
        val rig = Rig()
        rig.startTwoCueTracks()

        assertEquals(List(10) { 5_000 }, rig.offerFromFile(CUT_US - 10 * FRAME_US, 10))
    }

    @Test
    fun play_20c_the_buffer_after_the_announcement_keeps_the_next_tracks_gain() {
        val rig = Rig()
        rig.startTwoCueTracks()
        rig.offerFromFile(CUT_US - 50 * FRAME_US, 100)

        rig.sink.setOutputStreamOffsetUs(OFFSET_US)

        assertEquals(List(10) { 20_000 }, rig.offerFromFile(CUT_US + 50 * FRAME_US, 10))
    }

    @Test
    fun play_20c_a_next_track_that_does_not_continue_the_file_is_not_split_into_a_buffer() {
        val rig = Rig()
        rig.startTwoCueTracks()
        rig.sink.setNextGain(DOUBLE_DB, null)

        assertEquals(List(100) { 5_000 }, rig.offerFromFile(CUT_US - 50 * FRAME_US, 100))
    }

    @Test
    fun play_20c_a_stream_whose_format_is_unknown_is_not_split() {
        val rig = Rig()
        rig.startTwoCueTracks(configured = false)

        assertEquals(List(100) { 5_000 }, rig.offerFromFile(CUT_US - 50 * FRAME_US, 100))
    }

    @Test
    fun play_20c_a_live_gain_change_keeps_where_the_next_track_continues() {
        val rig = Rig()
        rig.startTwoCueTracks()

        rig.sink.setGains(DOUBLE_DB, HALF_DB)

        assertEquals(
            List(50) { 20_000 } + List(50) { 5_000 },
            rig.offerFromFile(CUT_US - 50 * FRAME_US, 100),
        )
    }

    @Test
    fun play_20c_a_split_buffer_the_output_takes_in_part_is_retried_from_the_same_copy() {
        val rig = Rig()
        rig.startTwoCueTracks()
        rig.probe.consumeBytesPerCall = 60 * Short.SIZE_BYTES
        val input = pcm16(*IntArray(100) { 10_000 })
        val presentationTimeUs = OFFSET_US + CUT_US - 50 * FRAME_US

        assertEquals(false, rig.sink.handleBuffer(input, presentationTimeUs, 1))
        rig.sink.setGains(HALF_DB, HALF_DB)
        assertEquals(true, rig.sink.handleBuffer(input, presentationTimeUs, 1))

        val offers = rig.probe.offers
        assertEquals(List(50) { 5_000 } + List(50) { 20_000 }, offers[0].samples)
        assertEquals(List(40) { 20_000 }, offers[1].samples)
    }

    @Test
    fun play_20c_unity_gain_on_both_sides_of_the_cut_passes_the_buffer_through() {
        val rig = Rig()
        rig.startTwoCueTracks()
        rig.sink.setGains(0.0, 0.0)
        val input = pcm16(*IntArray(100) { 10_000 })

        rig.sink.handleBuffer(input, OFFSET_US + CUT_US - 50 * FRAME_US, 1)

        assertSame(input, rig.probe.offers.single().buffer)
    }

    @Test
    fun play_20c_a_configuration_for_another_period_starts_the_next_stream_before_its_announcement() {
        val rig = Rig()
        rig.sink.startPlaylist(HALF_DB, DOUBLE_DB)
        rig.configureFor("first")
        rig.sink.setOutputStreamOffsetUs(OFFSET_US)
        assertEquals(5_000, rig.scaledAt(OFFSET_US + 100))

        // A new decoder starts for the next file: the sink is configured for its
        // period and offered its first buffer before the offset is announced.
        rig.configureFor("second")
        assertEquals(20_000, rig.scaledAt(OFFSET_US + 200))
        rig.sink.setOutputStreamOffsetUs(OFFSET_US + 1_000)
        assertEquals(20_000, rig.scaledAt(OFFSET_US + 300))
    }

    @Test
    fun play_20c_a_configuration_for_the_same_period_or_for_none_does_not_change_the_gain() {
        val rig = Rig()
        rig.sink.startPlaylist(HALF_DB, DOUBLE_DB)
        rig.configureFor("first")
        rig.sink.setOutputStreamOffsetUs(OFFSET_US)
        assertEquals(5_000, rig.scaledAt(OFFSET_US + 100))

        rig.configureFor("first")
        rig.configureFor(null)
        assertEquals(5_000, rig.scaledAt(OFFSET_US + 200))
    }

    @Test
    fun play_20c_a_flush_keeps_the_period_so_the_next_one_still_starts_the_next_stream() {
        val rig = Rig()
        rig.sink.startPlaylist(HALF_DB, DOUBLE_DB)
        rig.configureFor("first")
        rig.sink.setOutputStreamOffsetUs(OFFSET_US)
        assertEquals(5_000, rig.scaledAt(OFFSET_US + 100))

        // A seek inside the period flushes the sink and configures nothing anew.
        rig.sink.flush()
        rig.sink.setOutputStreamOffsetUs(OFFSET_US)
        assertEquals(5_000, rig.scaledAt(OFFSET_US + 50))

        rig.configureFor("second")
        assertEquals(20_000, rig.scaledAt(OFFSET_US + 200))
    }

    @Test
    fun play_20c_a_seek_back_into_the_first_period_keeps_the_first_tracks_gain() {
        val rig = Rig()
        rig.sink.startPlaylist(HALF_DB, DOUBLE_DB)
        rig.configureFor("first")
        rig.sink.setOutputStreamOffsetUs(OFFSET_US)
        rig.scaledAt(OFFSET_US + 100)
        rig.configureFor("second")
        assertEquals(20_000, rig.scaledAt(OFFSET_US + 200))

        rig.sink.flush()
        rig.sink.setOutputStreamOffsetUs(OFFSET_US)
        rig.configureFor("first")
        assertEquals(5_000, rig.scaledAt(OFFSET_US + 100))
    }
}
