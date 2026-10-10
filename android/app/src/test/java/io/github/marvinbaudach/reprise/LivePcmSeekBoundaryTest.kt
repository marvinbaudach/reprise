package io.github.marvinbaudach.reprise

import androidx.media3.common.C
import androidx.media3.common.Player
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * A seek is a decoded-stream boundary: the new passage can be far louder or quieter than the one
 * the gain was measured on, so the visualizer must measure it again (#1180).
 *
 * Media3 reports a seek as `onPositionDiscontinuity(SEEK)`, then `onIsPlayingChanged(false)` for
 * the buffering, then the audio sink flush, then `onIsPlayingChanged(true)`. Both flushes arrive
 * while the sink still reads "not playing" (measured on the phone: the playing change is
 * delivered 4 ms before the flush), so the flush guard alone never sees the seek.
 */
class LivePcmSeekBoundaryTest {
    private val consumer = ResetCountingConsumer()
    private val sink = LivePcmBufferSink().also {
        it.flush(48_000, 2, C.ENCODING_PCM_16BIT)
        it.attach(consumer)
    }

    @Test
    fun aSeekWhilePlayingResetsTheStreamWhenPlaybackResumes() {
        startPlaying()

        bufferThroughASeek(Player.DISCONTINUITY_REASON_SEEK)

        assertEquals("the seek must be a stream boundary", 1, consumer.streamResets)
        assertEquals("a boundary is not also a gap", 0, consumer.historyResets)
    }

    @Test
    fun aSeekWhilePausedResetsTheStreamAtTheNextResume() {
        startPlaying()
        sink.onIsPlayingChanged(false)
        consumer.clear()

        sink.onPositionDiscontinuity(position(), position(), Player.DISCONTINUITY_REASON_SEEK)
        sink.flush(48_000, 2, C.ENCODING_PCM_16BIT)
        sink.onIsPlayingChanged(true)

        assertEquals(1, consumer.streamResets)
        assertEquals(0, consumer.historyResets)
    }

    @Test
    fun aBufferingStallWithoutASeekStaysAGap() {
        startPlaying()

        sink.onIsPlayingChanged(false)
        sink.onIsPlayingChanged(true)

        assertEquals("a stall keeps the gain and the shape", 0, consumer.streamResets)
        assertEquals(1, consumer.historyResets)
    }

    @Test
    fun theSeekBoundaryIsSpentOnceAndTheNextStallIsAGapAgain() {
        startPlaying()
        bufferThroughASeek(Player.DISCONTINUITY_REASON_SEEK)
        consumer.clear()

        sink.onIsPlayingChanged(false)
        sink.onIsPlayingChanged(true)

        assertEquals(0, consumer.streamResets)
        assertEquals(1, consumer.historyResets)
    }

    @Test
    fun aFlushWhilePlayingAfterASeekResetsTheStreamOnlyOnce() {
        startPlaying()

        sink.onPositionDiscontinuity(position(), position(), Player.DISCONTINUITY_REASON_SEEK)
        sink.flush(48_000, 2, C.ENCODING_PCM_16BIT)
        sink.onIsPlayingChanged(false)
        sink.onIsPlayingChanged(true)

        assertEquals(1, consumer.streamResets)
    }

    @Test
    fun otherDiscontinuitiesAreNotSeeks() {
        // A song change is announced to the visualizer by its own call (`noteTrackChanged`), and
        // resetting the stream here as well would undo the shape a swipe hands over.
        val others = listOf(
            Player.DISCONTINUITY_REASON_AUTO_TRANSITION,
            Player.DISCONTINUITY_REASON_SEEK_ADJUSTMENT,
            Player.DISCONTINUITY_REASON_SKIP,
            Player.DISCONTINUITY_REASON_REMOVE,
            Player.DISCONTINUITY_REASON_INTERNAL,
        )
        for (reason in others) {
            consumer.clear()
            startPlaying()
            consumer.clear()

            bufferThroughASeek(reason)

            assertEquals("reason $reason", 0, consumer.streamResets)
            assertEquals("reason $reason", 1, consumer.historyResets)
            sink.onIsPlayingChanged(false)
        }
    }

    private fun startPlaying() {
        sink.onIsPlayingChanged(true)
        consumer.clear()
    }

    /** The order Media3 delivered on the phone for a seek made while playing. */
    private fun bufferThroughASeek(reason: Int) {
        sink.onPositionDiscontinuity(position(), position(), reason)
        sink.onIsPlayingChanged(false)
        sink.flush(48_000, 2, C.ENCODING_PCM_16BIT)
        sink.flush(48_000, 2, C.ENCODING_PCM_16BIT)
        sink.onIsPlayingChanged(true)
    }

    private fun position() = Player.PositionInfo(null, 0, null, null, 0, 0L, 0L, -1, -1)
}

private class ResetCountingConsumer : LivePcmConsumer {
    var streamResets = 0
    var historyResets = 0

    fun clear() {
        streamResets = 0
        historyResets = 0
    }

    override fun setPlaybackIntent(playbackIntended: Boolean) = Unit

    override fun ingestPcm16(
        bytes: ByteArray,
        byteCount: Int,
        sampleRateHz: Int,
        channelCount: Int,
    ) = Unit

    override fun resetAudioStream() {
        streamResets += 1
    }

    override fun resetAudioHistory() {
        historyResets += 1
    }
}
