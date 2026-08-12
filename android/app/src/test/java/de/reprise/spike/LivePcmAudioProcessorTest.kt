package de.reprise.spike

import androidx.media3.common.C
import androidx.media3.common.Player
import androidx.media3.common.TrackSelectionParameters
import androidx.media3.common.audio.AudioProcessor
import androidx.media3.common.audio.AudioProcessor.AudioFormat
import androidx.media3.exoplayer.audio.TeeAudioProcessor
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import kotlin.concurrent.thread
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class LivePcmAudioProcessorTest {
    @Test
    fun resumeResetNeverHoldsTheMonitorNeededByTheRenderThread() {
        val consumer = BlockingResetConsumer()
        val sink = LivePcmBufferSink()
        val pcm = stereoPcm16(
            left = shortArrayOf(1_000, -2_000, 3_000),
            right = shortArrayOf(-4_000, 5_000, -6_000),
        )
        sink.flush(48_000, 2, C.ENCODING_PCM_16BIT)
        sink.attach(consumer)

        val resumeThread = thread(name = "resume-reset") {
            sink.onIsPlayingChanged(true)
        }
        assertTrue(consumer.resetEntered.await(1, TimeUnit.SECONDS))
        val renderThread = thread(name = "pcm-render") {
            sink.handleBuffer(directBuffer(pcm))
        }

        try {
            assertTrue(
                "render thread waited for the application-thread reset",
                consumer.ingestEntered.await(1, TimeUnit.SECONDS),
            )
        } finally {
            consumer.allowResetToFinish.countDown()
            resumeThread.join()
            renderThread.join()
        }
    }

    @Test
    fun playbackIntentRemainsTrueWhenIsPlayingFallsFalseForBuffering() {
        val consumer = RecordingPcmConsumer()
        val sink = LivePcmBufferSink().apply { attach(consumer) }

        assertTrue(consumer.playbackIntentChanges.isEmpty())

        sink.onPlayWhenReadyChanged(true, Player.PLAY_WHEN_READY_CHANGE_REASON_USER_REQUEST)
        sink.onIsPlayingChanged(false)

        assertEquals(listOf(true), consumer.playbackIntentChanges)
    }

    @Test
    fun pcmArrivingAfterPlayerPauseIsDiscardedEvenAsABurst() {
        val consumer = RecordingPcmConsumer()
        val sink = LivePcmBufferSink().apply { attach(consumer) }
        val pcm = stereoPcm16(
            left = shortArrayOf(1_000, -2_000, 3_000),
            right = shortArrayOf(-4_000, 5_000, -6_000),
        )
        sink.flush(48_000, 2, C.ENCODING_PCM_16BIT)
        sink.onIsPlayingChanged(true)
        sink.handleBuffer(directBuffer(pcm))

        sink.onIsPlayingChanged(false)
        repeat(3) {
            sink.handleBuffer(directBuffer(pcm))
        }

        assertEquals(1, consumer.ingestCount)
    }

    @Test
    fun resumingAfterPausedFlushResetsHistoryBeforePcmRestarts() {
        val consumer = RecordingPcmConsumer()
        val sink = LivePcmBufferSink()
        val pcm = stereoPcm16(
            left = shortArrayOf(1_000, -2_000, 3_000),
            right = shortArrayOf(-4_000, 5_000, -6_000),
        )
        sink.flush(48_000, 2, C.ENCODING_PCM_16BIT)
        sink.attach(consumer)
        sink.onIsPlayingChanged(true)
        sink.handleBuffer(directBuffer(pcm))

        sink.onIsPlayingChanged(false)
        sink.flush(48_000, 2, C.ENCODING_PCM_16BIT)
        assertEquals(0, consumer.resetCount)
        assertEquals(1, consumer.historyResetCount)

        sink.onIsPlayingChanged(true)
        assertEquals(0, consumer.resetCount)
        assertEquals(2, consumer.historyResetCount)
        sink.handleBuffer(directBuffer(pcm))
        assertEquals(2, consumer.ingestCount)
    }

    @Test
    fun processorCopiesPcmBitForBitWhilePublishingTheSameBufferWithoutAnalysis() {
        val consumer = RecordingPcmConsumer()
        val sink = LivePcmBufferSink().apply { attach(consumer) }
        val processor = TeeAudioProcessor(sink)
        val format = AudioFormat(48_000, 2, C.ENCODING_PCM_16BIT)
        val expected = stereoPcm16(
            left = shortArrayOf(1_000, -2_000, 3_000),
            right = shortArrayOf(-4_000, 5_000, -6_000),
        )

        processor.configure(format)
        processor.flush(AudioProcessor.StreamMetadata.DEFAULT)
        sink.onIsPlayingChanged(true)
        processor.queueInput(directBuffer(expected))

        assertArrayEquals(expected, processor.output.toByteArray())
        assertArrayEquals(expected, consumer.bytes.copyOf(consumer.byteCount))
        assertEquals(48_000, consumer.sampleRateHz)
        assertEquals(2, consumer.channelCount)
        assertEquals(0, consumer.resetCount)
        assertEquals(1, consumer.historyResetCount)
    }

    @Test
    fun everyMedia3FlushWhilePlayingResetsTheAttachedLiveProcessor() {
        val consumer = RecordingPcmConsumer()
        val sink = LivePcmBufferSink().apply { attach(consumer) }
        val processor = TeeAudioProcessor(sink)
        processor.configure(AudioFormat(44_100, 1, C.ENCODING_PCM_16BIT))
        sink.onIsPlayingChanged(true)

        processor.flush(AudioProcessor.StreamMetadata.DEFAULT)
        processor.flush(AudioProcessor.StreamMetadata.DEFAULT)

        assertEquals(2, consumer.resetCount)
        assertEquals(1, consumer.historyResetCount)
    }

    @Test
    fun throwingVisualizerConsumerCannotInterruptPcmForwarding() {
        val sink = LivePcmBufferSink().apply { attach(ThrowingPcmConsumer()) }
        val processor = TeeAudioProcessor(sink)
        val expected = stereoPcm16(
            left = shortArrayOf(1_000, -2_000, 3_000),
            right = shortArrayOf(-4_000, 5_000, -6_000),
        )
        processor.configure(AudioFormat(48_000, 2, C.ENCODING_PCM_16BIT))
        processor.flush(AudioProcessor.StreamMetadata.DEFAULT)
        sink.onIsPlayingChanged(true)

        processor.queueInput(directBuffer(expected))

        assertArrayEquals(expected, processor.output.toByteArray())
    }

    @Test
    fun throwingVisualizerResetCannotEscapeFlushOrDetach() {
        val consumer = ThrowingResetConsumer()
        val sink = LivePcmBufferSink().apply { attach(consumer) }

        sink.flush(48_000, 2, C.ENCODING_PCM_16BIT)
        sink.detach(consumer)
    }

    @Test
    fun detachingVisualizerResetsItsLiveStreamBeforeRemovingIt() {
        val consumer = RecordingPcmConsumer()
        val sink = LivePcmBufferSink().apply { attach(consumer) }

        sink.detach(consumer)
        sink.flush(48_000, 2, C.ENCODING_PCM_16BIT)

        assertEquals(1, consumer.resetCount)
    }

    @Test
    fun livePcmPlaybackExplicitlyKeepsAudioOffloadDisabled() {
        assertEquals(
            TrackSelectionParameters.AudioOffloadPreferences.AUDIO_OFFLOAD_MODE_DISABLED,
            livePcmAudioOffloadPreferences().audioOffloadMode,
        )
    }
}

private class RecordingPcmConsumer : LivePcmConsumer {
    var bytes = ByteArray(0)
    var byteCount = 0
    var sampleRateHz = 0
    var channelCount = 0
    var resetCount = 0
    var historyResetCount = 0
    var ingestCount = 0
    val playbackIntentChanges = mutableListOf<Boolean>()

    override fun setPlaybackIntent(playbackIntended: Boolean) {
        playbackIntentChanges += playbackIntended
    }

    override fun ingestPcm16(
        bytes: ByteArray,
        byteCount: Int,
        sampleRateHz: Int,
        channelCount: Int,
    ) {
        ingestCount += 1
        this.bytes = bytes.copyOf()
        this.byteCount = byteCount
        this.sampleRateHz = sampleRateHz
        this.channelCount = channelCount
    }

    override fun resetAudioStream() {
        resetCount += 1
    }

    override fun resetAudioHistory() {
        historyResetCount += 1
    }
}

private class ThrowingPcmConsumer : LivePcmConsumer {
    override fun setPlaybackIntent(playbackIntended: Boolean) = Unit

    override fun resetAudioHistory() = Unit

    override fun ingestPcm16(
        bytes: ByteArray,
        byteCount: Int,
        sampleRateHz: Int,
        channelCount: Int,
    ): Nothing = error("visualizer ingest failed")

    override fun resetAudioStream() = Unit
}

private class ThrowingResetConsumer : LivePcmConsumer {
    override fun setPlaybackIntent(playbackIntended: Boolean) = Unit

    override fun resetAudioHistory(): Nothing = error("visualizer history reset failed")

    override fun ingestPcm16(
        bytes: ByteArray,
        byteCount: Int,
        sampleRateHz: Int,
        channelCount: Int,
    ) = Unit

    override fun resetAudioStream(): Nothing = error("visualizer reset failed")
}

private class BlockingResetConsumer : LivePcmConsumer {
    val resetEntered = CountDownLatch(1)
    val allowResetToFinish = CountDownLatch(1)
    val ingestEntered = CountDownLatch(1)

    override fun setPlaybackIntent(playbackIntended: Boolean) = Unit

    override fun resetAudioHistory() {
        resetEntered.countDown()
        allowResetToFinish.await()
    }

    override fun ingestPcm16(
        bytes: ByteArray,
        byteCount: Int,
        sampleRateHz: Int,
        channelCount: Int,
    ) {
        ingestEntered.countDown()
    }

    override fun resetAudioStream() = Unit
}

private fun stereoPcm16(left: ShortArray, right: ShortArray): ByteArray {
    require(left.size == right.size)
    return ByteBuffer.allocate(left.size * 4)
        .order(ByteOrder.LITTLE_ENDIAN)
        .apply {
            left.indices.forEach { frame ->
                putShort(left[frame])
                putShort(right[frame])
            }
        }
        .array()
}

private fun directBuffer(bytes: ByteArray): ByteBuffer = ByteBuffer
    .allocateDirect(bytes.size)
    .order(ByteOrder.LITTLE_ENDIAN)
    .put(bytes)
    .flip() as ByteBuffer

private fun ByteBuffer.toByteArray(): ByteArray =
    ByteArray(remaining()).also(::get)
