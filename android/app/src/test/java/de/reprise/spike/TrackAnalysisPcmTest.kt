package de.reprise.spike

import android.media.AudioFormat
import java.nio.ByteBuffer
import java.nio.ByteOrder
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test

class TrackAnalysisPcmTest {
    @Test
    fun signed16BitPcmIsConvertedToInterleavedFloatAtTheDecoderEdge() {
        val bytes = ByteBuffer.allocate(6).order(ByteOrder.nativeOrder())
            .putShort(Short.MIN_VALUE)
            .putShort(0)
            .putShort(Short.MAX_VALUE)
            .flip() as ByteBuffer
        val chunks = mutableListOf<List<Float>>()

        PcmOutputPump.push(
            buffer = bytes,
            pcmEncoding = AudioFormat.ENCODING_PCM_16BIT,
            sampleRateHz = 48_000,
            channelCount = 1,
            sink = { samples, _, _ -> chunks += samples },
        )

        assertEquals(listOf(-1f, 0f, 32767f / 32768f), chunks.single())
    }

    @Test
    fun floatPcmKeepsItsInterleavedValues() {
        val bytes = ByteBuffer.allocate(12).order(ByteOrder.nativeOrder())
            .putFloat(-0.5f)
            .putFloat(0.25f)
            .putFloat(1f)
            .flip() as ByteBuffer
        val chunks = mutableListOf<List<Float>>()

        PcmOutputPump.push(
            bytes,
            AudioFormat.ENCODING_PCM_FLOAT,
            sampleRateHz = 48_000,
            channelCount = 1,
        ) { samples, _, _ -> chunks += samples }

        assertEquals(listOf(-0.5f, 0.25f, 1f), chunks.single())
    }

    @Test
    fun outputLargerThanOneSecondIsSplitWithoutDroppingAFrame() {
        val bytes = ByteBuffer.allocate(12).order(ByteOrder.nativeOrder())
        repeat(6) { sample -> bytes.putShort((sample * 1_000).toShort()) }
        bytes.flip()
        val chunks = mutableListOf<List<Float>>()

        PcmOutputPump.push(
            bytes,
            AudioFormat.ENCODING_PCM_16BIT,
            sampleRateHz = 4,
            channelCount = 1,
        ) { samples, _, _ -> chunks += samples }

        assertEquals(listOf(4, 2), chunks.map(List<Float>::size))
        assertEquals(6, chunks.sumOf(List<Float>::size))
    }

    @Test
    fun anUnsupportedCodecPcmFormatFailsInsteadOfInventingSamples() {
        val bytes = ByteBuffer.allocate(4)

        assertThrows(UnsupportedPcmFormatException::class.java) {
            PcmOutputPump.push(bytes, AudioFormat.ENCODING_PCM_8BIT, 48_000, 1) { _, _, _ -> }
        }
    }
}
