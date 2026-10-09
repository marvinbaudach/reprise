package io.github.marvinbaudach.reprise.library

import androidx.annotation.OptIn
import androidx.media3.common.C
import androidx.media3.common.util.UnstableApi
import androidx.media3.extractor.ExtractorInput
import androidx.media3.extractor.ForwardingExtractorInput
import java.io.EOFException

private const val STREAM_MARKER_SIZE = 4
private const val STREAM_INFO_FIELDS_END = 26
private const val TOTAL_SAMPLES_FIRST_BYTE = 21L
private const val TOTAL_SAMPLES_LAST_BYTE = 25L
private const val ASSUMED_COMPRESSION_RATIO = 0.25
private const val MAX_36_BIT = (1L shl 36) - 1L

/**
 * Masks understated FLAC STREAMINFO sample counts for MTP-68 open-ended playback.
 *
 * Media3 rejects frames beyond the declared total. For FLAC without a SEEKTABLE,
 * it also uses that total as the interpolation seeker's ceiling. MTP-68 therefore
 * raises an understated count to a conservative estimate from the file length,
 * while preserving every other byte and never lowering the declared count.
 *
 * The 4:1 compression assumption keeps the ceiling above typical audio without
 * making interpolation impractical. Audio compressing better than 4:1 can still
 * end beyond the estimate when its header is understated.
 */
@OptIn(UnstableApi::class)
internal class FlacStreamInfoMaskingExtractorInput(
    private val source: ExtractorInput,
) : ForwardingExtractorInput(source) {
    private var inspectedStreamInfo = false
    private var totalSamplesToPresent: Long? = null

    internal fun wraps(input: ExtractorInput): Boolean = source === input

    override fun read(target: ByteArray, offset: Int, length: Int): Int {
        val start = position
        val totalSamples = streamInfoTotalSamples()
        return super.read(target, offset, length).also { bytesRead ->
            if (totalSamples != null && bytesRead > 0) {
                maskTotalSamples(target, offset, bytesRead, start, totalSamples)
            }
        }
    }

    override fun readFully(
        target: ByteArray,
        offset: Int,
        length: Int,
        allowEndOfInput: Boolean,
    ): Boolean {
        val start = position
        val totalSamples = streamInfoTotalSamples()
        return super.readFully(target, offset, length, allowEndOfInput).also { completed ->
            if (totalSamples != null && completed) {
                maskTotalSamples(target, offset, length, start, totalSamples)
            }
        }
    }

    override fun readFully(target: ByteArray, offset: Int, length: Int) {
        val start = position
        val totalSamples = streamInfoTotalSamples()
        super.readFully(target, offset, length)
        if (totalSamples != null) maskTotalSamples(target, offset, length, start, totalSamples)
    }

    override fun peek(target: ByteArray, offset: Int, length: Int): Int {
        val start = peekPosition
        val totalSamples = streamInfoTotalSamples()
        return super.peek(target, offset, length).also { bytesRead ->
            if (totalSamples != null && bytesRead > 0) {
                maskTotalSamples(target, offset, bytesRead, start, totalSamples)
            }
        }
    }

    override fun peekFully(
        target: ByteArray,
        offset: Int,
        length: Int,
        allowEndOfInput: Boolean,
    ): Boolean {
        val start = peekPosition
        val totalSamples = streamInfoTotalSamples()
        return super.peekFully(target, offset, length, allowEndOfInput).also { completed ->
            if (totalSamples != null && completed) {
                maskTotalSamples(target, offset, length, start, totalSamples)
            }
        }
    }

    override fun peekFully(target: ByteArray, offset: Int, length: Int) {
        val start = peekPosition
        val totalSamples = streamInfoTotalSamples()
        super.peekFully(target, offset, length)
        if (totalSamples != null) maskTotalSamples(target, offset, length, start, totalSamples)
    }

    override fun skip(length: Int): Int {
        streamInfoTotalSamples()
        return super.skip(length)
    }

    override fun skipFully(length: Int, allowEndOfInput: Boolean): Boolean {
        streamInfoTotalSamples()
        return super.skipFully(length, allowEndOfInput)
    }

    override fun skipFully(length: Int) {
        streamInfoTotalSamples()
        super.skipFully(length)
    }

    private fun streamInfoTotalSamples(): Long? {
        if (inspectedStreamInfo) return totalSamplesToPresent
        if (position != 0L) return null

        val originalPeekPosition = peekPosition
        resetPeekPosition()
        val streamInfo = ByteArray(STREAM_INFO_FIELDS_END)
        val streamInfoRead = try {
            try {
                super.peekFully(streamInfo, 0, streamInfo.size, true)
            } catch (_: EOFException) {
                false
            }
        } finally {
            resetPeekPosition()
            restorePeekPosition(originalPeekPosition)
        }
        inspectedStreamInfo = true
        if (!streamInfoRead || !streamInfo.copyOf(STREAM_MARKER_SIZE).contentEquals(FLAC_MARKER)) {
            return null
        }
        return estimatedTotalSamples(streamInfo).also { totalSamplesToPresent = it }
    }

    private fun estimatedTotalSamples(streamInfo: ByteArray): Long {
        val declared = ((streamInfo[21].toLong() and 0x0F) shl 32) or
            ((streamInfo[22].toLong() and 0xFF) shl 24) or
            ((streamInfo[23].toLong() and 0xFF) shl 16) or
            ((streamInfo[24].toLong() and 0xFF) shl 8) or
            (streamInfo[25].toLong() and 0xFF)
        val channels = ((streamInfo[20].toInt() and 0x0E) ushr 1) + 1
        val bitsPerSample = (((streamInfo[20].toInt() and 0x01) shl 4) or
            ((streamInfo[21].toInt() and 0xF0) ushr 4)) + 1
        val fileLength = length
        if (
            fileLength == C.LENGTH_UNSET.toLong() ||
            fileLength <= 0L ||
            channels !in 1..8 ||
            bitsPerSample !in 4..32
        ) {
            return declared
        }

        val bytesPerSampleFrame = channels * bitsPerSample / 8.0
        val estimate = fileLength / (ASSUMED_COMPRESSION_RATIO * bytesPerSampleFrame)
        if (!estimate.isFinite() || estimate < 0.0) return declared
        return maxOf(declared, minOf(MAX_36_BIT, estimate.toLong()))
    }

    private fun restorePeekPosition(targetPosition: Long) {
        var remaining = targetPosition - position
        while (remaining > 0) {
            val step = minOf(remaining, Int.MAX_VALUE.toLong()).toInt()
            super.advancePeekPosition(step)
            remaining -= step
        }
    }

    private fun maskTotalSamples(
        target: ByteArray,
        offset: Int,
        length: Int,
        absoluteStart: Long,
        totalSamples: Long,
    ) {
        val absoluteEnd = absoluteStart + length
        for (position in TOTAL_SAMPLES_FIRST_BYTE..TOTAL_SAMPLES_LAST_BYTE) {
            if (position !in absoluteStart until absoluteEnd) continue
            val targetIndex = offset + (position - absoluteStart).toInt()
            target[targetIndex] = if (position == TOTAL_SAMPLES_FIRST_BYTE) {
                ((target[targetIndex].toInt() and 0xF0) or
                    ((totalSamples ushr 32).toInt() and 0x0F)).toByte()
            } else {
                (totalSamples ushr ((TOTAL_SAMPLES_LAST_BYTE - position) * 8).toInt()).toByte()
            }
        }
    }

    private companion object {
        val FLAC_MARKER = byteArrayOf(
            'f'.code.toByte(),
            'L'.code.toByte(),
            'a'.code.toByte(),
            'C'.code.toByte(),
        )
    }
}
