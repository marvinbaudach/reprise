package de.reprise.spike

import androidx.media3.common.C
import androidx.media3.common.TrackSelectionParameters
import androidx.media3.common.audio.AudioProcessor
import androidx.media3.common.audio.AudioProcessor.AudioFormat
import androidx.media3.exoplayer.audio.TeeAudioProcessor
import java.nio.ByteBuffer
import java.nio.ByteOrder
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Test

class LivePcmAudioProcessorTest {
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
        processor.queueInput(directBuffer(expected))

        assertArrayEquals(expected, processor.output.toByteArray())
        assertArrayEquals(expected, consumer.bytes.copyOf(consumer.byteCount))
        assertEquals(48_000, consumer.sampleRateHz)
        assertEquals(2, consumer.channelCount)
        assertEquals(1, consumer.resetCount)
    }

    @Test
    fun everyMedia3FlushResetsTheAttachedLiveProcessor() {
        val consumer = RecordingPcmConsumer()
        val processor = TeeAudioProcessor(LivePcmBufferSink().apply { attach(consumer) })
        processor.configure(AudioFormat(44_100, 1, C.ENCODING_PCM_16BIT))

        processor.flush(AudioProcessor.StreamMetadata.DEFAULT)
        processor.flush(AudioProcessor.StreamMetadata.DEFAULT)

        assertEquals(2, consumer.resetCount)
    }

    @Test
    fun throwingVisualizerConsumerCannotInterruptPcmForwarding() {
        val processor = TeeAudioProcessor(
            LivePcmBufferSink().apply { attach(ThrowingPcmConsumer()) },
        )
        val expected = stereoPcm16(
            left = shortArrayOf(1_000, -2_000, 3_000),
            right = shortArrayOf(-4_000, 5_000, -6_000),
        )
        processor.configure(AudioFormat(48_000, 2, C.ENCODING_PCM_16BIT))
        processor.flush(AudioProcessor.StreamMetadata.DEFAULT)

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

    override fun ingestPcm16(
        bytes: ByteArray,
        byteCount: Int,
        sampleRateHz: Int,
        channelCount: Int,
    ) {
        this.bytes = bytes.copyOf()
        this.byteCount = byteCount
        this.sampleRateHz = sampleRateHz
        this.channelCount = channelCount
    }

    override fun resetAudioStream() {
        resetCount += 1
    }
}

private class ThrowingPcmConsumer : LivePcmConsumer {
    override fun ingestPcm16(
        bytes: ByteArray,
        byteCount: Int,
        sampleRateHz: Int,
        channelCount: Int,
    ): Nothing = error("visualizer ingest failed")

    override fun resetAudioStream() = Unit
}

private class ThrowingResetConsumer : LivePcmConsumer {
    override fun ingestPcm16(
        bytes: ByteArray,
        byteCount: Int,
        sampleRateHz: Int,
        channelCount: Int,
    ) = Unit

    override fun resetAudioStream(): Nothing = error("visualizer reset failed")
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
