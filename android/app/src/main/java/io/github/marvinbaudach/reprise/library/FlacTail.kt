package io.github.marvinbaudach.reprise.library

import android.net.Uri
import androidx.annotation.OptIn
import androidx.media3.common.C
import androidx.media3.common.util.ParsableByteArray
import androidx.media3.common.util.UnstableApi
import androidx.media3.datasource.DataSource
import androidx.media3.datasource.DataSourceUtil
import androidx.media3.datasource.DataSpec
import androidx.media3.extractor.FlacFrameReader
import androidx.media3.extractor.FlacStreamMetadata
import java.io.IOException

private const val STREAM_INFO_BODY_OFFSET = 8
private const val STREAM_INFO_BODY_SIZE = 34
private const val TOTAL_SAMPLES_HIGH_BYTE = 13
private const val TOTAL_SAMPLES_FIRST_LOW_BYTE = 14
private const val TOTAL_SAMPLES_LAST_LOW_BYTE = 17
private const val FRAME_SYNC_HIGH_BYTE = 0xFF
private const val FRAME_SYNC_LOW_BYTE_FIXED = 0xF8
private const val FRAME_SYNC_LOW_BYTE_VARIABLE = 0xF9
private const val FRAME_HEADER_FIXED_BYTES = 4
private const val BLOCK_SIZE_KEY_BYTE = 2
private const val UTF8_NUMBER_BYTE = 4
private const val MIN_TAIL_WINDOW_BYTES = 64 * 1024
private const val UNKNOWN_FRAME_TAIL_WINDOW_BYTES = 256 * 1024
private const val MAX_TAIL_WINDOW_BYTES = 4 * 1024 * 1024
private const val MAX_FRAME_SIZE_FIRST_BYTE = 15 // File offset of the 24-bit maximum frame size.

/** Reads [length] bytes of the file being extracted from [position], or null when it cannot. */
internal fun interface FlacTailReader {
    fun read(position: Long, length: Int): ByteArray?
}

/** Opens its own connection to [uri], because an extractor's input can only move forward. */
@OptIn(UnstableApi::class)
internal class DataSourceFlacTailReader(
    private val dataSourceFactory: DataSource.Factory,
    private val uri: Uri,
) : FlacTailReader {
    override fun read(position: Long, length: Int): ByteArray? {
        val source = dataSourceFactory.createDataSource()
        return try {
            source.open(
                DataSpec.Builder()
                    .setUri(uri)
                    .setPosition(position)
                    .setLength(length.toLong())
                    .build(),
            )
            val bytes = ByteArray(length)
            var filled = 0
            while (filled < length) {
                val read = source.read(bytes, filled, length - filled)
                if (read == C.RESULT_END_OF_INPUT) break
                filled += read
            }
            bytes.copyOf(filled)
        } catch (_: IOException) {
            null
        } finally {
            DataSourceUtil.closeQuietly(source)
        }
    }
}

/**
 * The sample count a FLAC file really holds, read from its last frame header instead of from
 * STREAMINFO, which a file can understate. Null when the tail cannot be read or holds no frame.
 *
 * [streamInfo] is the first [STREAM_INFO_FIELDS_END] bytes of the file; [fileLength] its size.
 */
@OptIn(UnstableApi::class)
internal fun flacSamplesFromLastFrame(
    streamInfo: ByteArray,
    fileLength: Long,
    tailReader: FlacTailReader,
): Long? {
    val metadata = streamInfoWithoutTotal(streamInfo)
    val window = tailWindowBytes(streamInfo)
    val tailStart = maxOf(STREAM_INFO_FIELDS_END.toLong(), fileLength - window)
    val tailLength = (fileLength - tailStart).toInt()
    if (tailLength <= 0) return null
    val tail = tailReader.read(tailStart, tailLength) ?: return null
    return lastFrameEnd(tail, metadata)
}

/** A copy of STREAMINFO with the total zeroed, so Media3 accepts a header of any sample number. */
@OptIn(UnstableApi::class)
private fun streamInfoWithoutTotal(streamInfo: ByteArray): FlacStreamMetadata {
    val body = ByteArray(STREAM_INFO_BODY_SIZE)
    streamInfo.copyInto(body, 0, STREAM_INFO_BODY_OFFSET, STREAM_INFO_FIELDS_END)
    body[TOTAL_SAMPLES_HIGH_BYTE] = (body[TOTAL_SAMPLES_HIGH_BYTE].toInt() and 0xF0).toByte()
    for (index in TOTAL_SAMPLES_FIRST_LOW_BYTE..TOTAL_SAMPLES_LAST_LOW_BYTE) body[index] = 0
    return FlacStreamMetadata(body, 0)
}

private fun tailWindowBytes(streamInfo: ByteArray): Int {
    val maxFrameSize = (0 until 3).fold(0) { size, index ->
        (size shl 8) or (streamInfo[MAX_FRAME_SIZE_FIRST_BYTE + index].toInt() and 0xFF)
    }
    if (maxFrameSize == 0) return UNKNOWN_FRAME_TAIL_WINDOW_BYTES
    return (2 * maxFrameSize).coerceIn(MIN_TAIL_WINDOW_BYTES, MAX_TAIL_WINDOW_BYTES)
}

@OptIn(UnstableApi::class)
private fun lastFrameEnd(tail: ByteArray, metadata: FlacStreamMetadata): Long? {
    val sampleNumber = FlacFrameReader.SampleNumberHolder()
    for (offset in tail.size - FRAME_HEADER_FIXED_BYTES downTo 0) {
        if (tail[offset].toInt() and 0xFF != FRAME_SYNC_HIGH_BYTE) continue
        val syncLow = tail[offset + 1].toInt() and 0xFF
        if (syncLow != FRAME_SYNC_LOW_BYTE_FIXED && syncLow != FRAME_SYNC_LOW_BYTE_VARIABLE) continue
        val marker = (FRAME_SYNC_HIGH_BYTE shl 8) or syncLow
        val blockSize = try {
            blockSizeOfValidHeader(tail, offset, marker, metadata, sampleNumber)
        } catch (_: IndexOutOfBoundsException) {
            null // A header cut off by the end of the file is not a frame.
        } ?: continue
        return sampleNumber.sampleNumber + blockSize
    }
    return null
}

@OptIn(UnstableApi::class)
private fun blockSizeOfValidHeader(
    tail: ByteArray,
    offset: Int,
    marker: Int,
    metadata: FlacStreamMetadata,
    sampleNumber: FlacFrameReader.SampleNumberHolder,
): Int? {
    val data = ParsableByteArray(tail)
    data.position = offset
    if (!FlacFrameReader.checkAndReadFrameHeader(data, metadata, marker, sampleNumber)) return null
    val blockSizeKey = (tail[offset + BLOCK_SIZE_KEY_BYTE].toInt() and 0xFF) ushr 4
    data.position = offset + UTF8_NUMBER_BYTE + utf8Length(tail[offset + UTF8_NUMBER_BYTE])
    return FlacFrameReader.readFrameBlockSizeSamplesFromKey(data, blockSizeKey).takeIf { it > 0 }
}

/** The byte length of a UTF-8 coded number from its first byte; FLAC allows up to seven. */
private fun utf8Length(first: Byte): Int =
    maxOf(1, Integer.numberOfLeadingZeros((first.toInt() and 0xFF).inv() shl 24))
