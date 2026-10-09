package io.github.marvinbaudach.reprise.library

import androidx.media3.common.C
import androidx.media3.common.DataReader
import androidx.media3.extractor.DefaultExtractorInput
import androidx.media3.extractor.ExtractorInput
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class FlacStreamInfoMaskingExtractorInputTest {
    @Test
    fun mtp_68_flac_total_samples_is_masked_through_every_read_path() {
        val source = flacStreamInfo()
        val expected = source.clone().apply { writeTotalSamples(ESTIMATED_SAMPLES) }

        assertArrayEquals(expected, drain(source, ExtractorInput::read))
        assertArrayEquals(expected, drainFully(source, ExtractorInput::readFully))
        assertArrayEquals(expected, drainFullyAllowingEnd(source, ExtractorInput::readFully))
        assertArrayEquals(expected, drain(source, ExtractorInput::peek))
        assertArrayEquals(expected, drainFully(source, ExtractorInput::peekFully))
        assertArrayEquals(expected, drainFullyAllowingEnd(source, ExtractorInput::peekFully))

        val afterSkip = maskingInput(source)
        while (afterSkip.position < 21L) {
            assertTrue(afterSkip.skip((21L - afterSkip.position).toInt()) > 0)
        }
        assertEquals(21L, afterSkip.position)
        val readAfterSkip = ByteArray(5).also { afterSkip.readFully(it, 0, it.size) }
        assertArrayEquals(expected.copyOfRange(21, 26), readAfterSkip)

        val afterPeekAdvance = maskingInput(source)
        afterPeekAdvance.advancePeekPosition(21)
        val readAfterPeekAdvance = ByteArray(5).also { afterPeekAdvance.peekFully(it, 0, it.size) }
        assertArrayEquals(expected.copyOfRange(21, 26), readAfterPeekAdvance)
        assertEquals(0L, afterPeekAdvance.position)
        assertEquals(26L, afterPeekAdvance.peekPosition)
    }

    @Test
    fun mtp_68_a_non_flac_stream_passes_through_byte_identical() {
        val source = flacStreamInfo().apply { this[0] = 'O'.code.toByte() }

        assertArrayEquals(source, drain(source, ExtractorInput::read))
        assertArrayEquals(source, drainFully(source, ExtractorInput::readFully))
        assertArrayEquals(source, drainFullyAllowingEnd(source, ExtractorInput::readFully))
        assertArrayEquals(source, drain(source, ExtractorInput::peek))
        assertArrayEquals(source, drainFully(source, ExtractorInput::peekFully))
        assertArrayEquals(source, drainFullyAllowingEnd(source, ExtractorInput::peekFully))
    }

    @Test
    fun mtp_68_unknown_length_keeps_the_declared_total_samples() {
        val source = flacStreamInfo()

        assertArrayEquals(source, drain(source, ExtractorInput::read, C.LENGTH_UNSET.toLong()))
    }

    @Test
    fun mtp_68_invalid_streaminfo_fields_keep_the_declared_total_samples() {
        val source = flacStreamInfo().apply {
            this[20] = (this[20].toInt() and 0xFE).toByte()
            this[21] = (this[21].toInt() and 0x0F).toByte() // One bit per sample is invalid FLAC.
        }

        assertArrayEquals(source, drain(source, ExtractorInput::read))
    }

    @Test
    fun mtp_68_the_estimate_never_lowers_the_declared_total_samples() {
        val source = flacStreamInfo().apply { writeTotalSamples(DECLARED_ABOVE_ESTIMATE) }

        assertArrayEquals(source, drain(source, ExtractorInput::read))
    }

    private fun drain(
        source: ByteArray,
        operation: ExtractorInput.(ByteArray, Int, Int) -> Int,
        inputLength: Long = FILE_LENGTH,
    ): ByteArray {
        val input = maskingInput(source, inputLength)
        val output = ByteArray(source.size)
        var offset = 0
        var chunkIndex = 0
        while (offset < output.size) {
            val requested = minOf(ODD_CHUNK_SIZES[chunkIndex % ODD_CHUNK_SIZES.size], output.size - offset)
            val read = input.operation(output, offset, requested)
            check(read > 0)
            offset += read
            chunkIndex += 1
        }
        return output
    }

    private fun drainFully(
        source: ByteArray,
        operation: ExtractorInput.(ByteArray, Int, Int) -> Unit,
    ): ByteArray {
        val input = maskingInput(source)
        val output = ByteArray(source.size)
        var offset = 0
        var chunkIndex = 0
        while (offset < output.size) {
            val requested = minOf(ODD_CHUNK_SIZES[chunkIndex % ODD_CHUNK_SIZES.size], output.size - offset)
            input.operation(output, offset, requested)
            offset += requested
            chunkIndex += 1
        }
        return output
    }

    private fun drainFullyAllowingEnd(
        source: ByteArray,
        operation: ExtractorInput.(ByteArray, Int, Int, Boolean) -> Boolean,
    ): ByteArray {
        val input = maskingInput(source)
        val output = ByteArray(source.size)
        var offset = 0
        var chunkIndex = 0
        while (offset < output.size) {
            val requested = minOf(ODD_CHUNK_SIZES[chunkIndex % ODD_CHUNK_SIZES.size], output.size - offset)
            assertTrue(input.operation(output, offset, requested, true))
            offset += requested
            chunkIndex += 1
        }
        return output
    }

    private fun maskingInput(source: ByteArray, inputLength: Long = FILE_LENGTH): ExtractorInput {
        var offset = 0
        val reader = DataReader { target, targetOffset, length ->
            if (offset == source.size) {
                -1
            } else {
                val read = minOf(length, source.size - offset)
                source.copyInto(target, targetOffset, offset, offset + read)
                offset += read
                read
            }
        }
        return FlacStreamInfoMaskingExtractorInput(
            DefaultExtractorInput(reader, 0, inputLength),
        )
    }

    private fun flacStreamInfo(): ByteArray = ByteArray(40) { index -> index.toByte() }.apply {
        "fLaC".encodeToByteArray().copyInto(this)
        val packed = (8_000L shl 44) or
            (1L shl 41) or // Two channels.
            (15L shl 36) or // 16 bits per sample.
            DECLARED_SAMPLES
        for (index in 0 until 8) {
            this[18 + index] = (packed ushr (56 - index * 8)).toByte()
        }
    }

    private fun ByteArray.writeTotalSamples(totalSamples: Long) {
        this[21] = ((this[21].toInt() and 0xF0) or ((totalSamples ushr 32).toInt() and 0x0F)).toByte()
        for (index in 22..25) {
            this[index] = (totalSamples ushr ((25 - index) * 8)).toByte()
        }
    }

    private companion object {
        const val FILE_LENGTH = 10_000L
        const val DECLARED_SAMPLES = 1_000L
        const val DECLARED_ABOVE_ESTIMATE = 20_000L
        const val ESTIMATED_SAMPLES = 10_000L
        val ODD_CHUNK_SIZES = intArrayOf(1, 3, 5, 7)
    }
}
