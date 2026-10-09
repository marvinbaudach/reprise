package io.github.marvinbaudach.reprise

import java.io.BufferedOutputStream
import java.io.File
import java.io.FileOutputStream

internal object TestFlacFile {
    const val SAMPLE_RATE = 8_000
    const val BLOCK_SIZE = 4_096
    const val DECLARED_SAMPLES = 60L * SAMPLE_RATE
    const val TRUE_SAMPLES = 1_172L * BLOCK_SIZE

    fun writeTenMinuteUnderstated(file: File): File {
        return writeFile(file, BLOCK_SIZE, 1_172, DECLARED_SAMPLES)
    }

    fun writeTenMinuteAccurate(file: File): File {
        return writeFile(file, BLOCK_SIZE, 1_172, TRUE_SAMPLES)
    }

    /**
     * The same ten minutes as [writeTenMinuteUnderstated], but every frame is a CONSTANT
     * subframe, so the file compresses about 800:1 the way a pure tone or silence does.
     */
    fun writeTenMinuteCompressibleUnderstated(file: File): File {
        return writeFile(file, BLOCK_SIZE, 1_172, DECLARED_SAMPLES, compressible = true)
    }

    fun withMetadataPadding(source: ByteArray, paddingSize: Int): ByteArray {
        var offset = 4
        while (offset + 4 <= source.size) {
            val isLast = source[offset].toInt() and 0x80 != 0
            val blockLength = ((source[offset + 1].toInt() and 0xFF) shl 16) or
                ((source[offset + 2].toInt() and 0xFF) shl 8) or
                (source[offset + 3].toInt() and 0xFF)
            val blockEnd = offset + 4 + blockLength
            check(blockEnd <= source.size) { "invalid FLAC metadata block length" }
            if (isLast) {
                val padded = ByteArray(source.size + 4 + paddingSize)
                source.copyInto(padded, endIndex = blockEnd)
                padded[offset] = (padded[offset].toInt() and 0x7F).toByte()
                padded[blockEnd] = 0x81.toByte() // Last metadata block, PADDING.
                padded[blockEnd + 1] = (paddingSize ushr 16).toByte()
                padded[blockEnd + 2] = (paddingSize ushr 8).toByte()
                padded[blockEnd + 3] = paddingSize.toByte()
                source.copyInto(padded, blockEnd + 4 + paddingSize, blockEnd)
                return padded
            }
            offset = blockEnd
        }
        error("FLAC fixture has no last metadata block")
    }

    private fun writeFile(
        file: File,
        blockSize: Int,
        frameCount: Int,
        declaredSamples: Long,
        compressible: Boolean = false,
    ): File {
        BufferedOutputStream(FileOutputStream(file)).use { output ->
            output.write("fLaC".encodeToByteArray())
            output.write(byteArrayOf(0x80.toByte(), 0, 0, 34))
            val subframeBytes = if (compressible) CONSTANT_SUBFRAME_BYTES else blockSize * 2 + 1
            val minFrameSize = frameSize(blockSize, frameNumberBytes = 1, subframeBytes)
            val maxFrameSize = frameSize(
                blockSize,
                frameNumberBytes = if (frameCount > 128) 2 else 1,
                subframeBytes,
            )
            output.write(streamInfo(blockSize, minFrameSize, maxFrameSize, declaredSamples))

            var noise = 0x6D2B79F5.toInt()
            repeat(frameCount) { frameNumber ->
                val header = frameHeader(frameNumber, blockSize)
                val frame = ByteArray(header.size + subframeBytes)
                header.copyInto(frame)
                // CONSTANT (0x00) or VERBATIM (0x02) subframe, no wasted bits.
                frame[header.size] = if (compressible) 0x00 else 0x02
                var offset = header.size + 1
                if (compressible) frame[offset + 1] = 0x40 // One constant 16-bit sample value.
                repeat(if (compressible) 0 else blockSize) {
                    noise = noise xor (noise shl 13)
                    noise = noise xor (noise ushr 17)
                    noise = noise xor (noise shl 5)
                    frame[offset++] = (noise ushr 24).toByte()
                    frame[offset++] = (noise ushr 16).toByte()
                }
                output.write(frame)
                val crc = crc16(frame)
                output.write(crc ushr 8)
                output.write(crc)
            }
        }
        return file
    }

    private fun streamInfo(
        blockSize: Int,
        minFrameSize: Int,
        maxFrameSize: Int,
        declaredSamples: Long,
    ): ByteArray = ByteArray(34).apply {
        write16(0, blockSize)
        write16(2, blockSize)
        write24(4, minFrameSize)
        write24(7, maxFrameSize)
        val packed = (SAMPLE_RATE.toLong() shl 44) or
            (15L shl 36) or // mono is zero; 16 bits per sample is 15.
            declaredSamples
        for (index in 0 until 8) {
            this[10 + index] = (packed ushr (56 - index * 8)).toByte()
        }
        // An all-zero MD5 is the FLAC representation for an unavailable signature.
    }

    private fun frameHeader(frameNumber: Int, blockSize: Int): ByteArray {
        val number = utf8(frameNumber)
        val (blockSizeCode, blockSizeExtra) = when (blockSize) {
            BLOCK_SIZE -> 0xC to byteArrayOf()
            else -> 0x7 to byteArrayOf(
                ((blockSize - 1) ushr 8).toByte(),
                (blockSize - 1).toByte(),
            )
        }
        val withoutCrc = byteArrayOf(
            0xFF.toByte(),
            0xF8.toByte(), // Sync, reserved bit zero, fixed-block strategy.
            ((blockSizeCode shl 4) or 0x04).toByte(),
            0x08, // Mono, 16-bit, reserved bit zero.
            *number,
            *blockSizeExtra,
        )
        return withoutCrc + crc8(withoutCrc).toByte()
    }

    private fun utf8(value: Int): ByteArray = when {
        value < 0x80 -> byteArrayOf(value.toByte())
        value < 0x800 -> byteArrayOf(
            (0xC0 or (value ushr 6)).toByte(),
            (0x80 or (value and 0x3F)).toByte(),
        )
        else -> error("fixture frame number exceeds two-byte UTF-8: $value")
    }

    private fun crc8(bytes: ByteArray): Int {
        var crc = 0
        for (byte in bytes) {
            crc = crc xor (byte.toInt() and 0xFF)
            repeat(8) {
                crc = if (crc and 0x80 != 0) (crc shl 1) xor 0x07 else crc shl 1
                crc = crc and 0xFF
            }
        }
        return crc
    }

    private fun crc16(bytes: ByteArray): Int {
        var crc = 0
        for (byte in bytes) {
            crc = crc xor ((byte.toInt() and 0xFF) shl 8)
            repeat(8) {
                crc = if (crc and 0x8000 != 0) (crc shl 1) xor 0x8005 else crc shl 1
                crc = crc and 0xFFFF
            }
        }
        return crc
    }

    private fun ByteArray.write16(offset: Int, value: Int) {
        this[offset] = (value ushr 8).toByte()
        this[offset + 1] = value.toByte()
    }

    private fun ByteArray.write24(offset: Int, value: Int) {
        this[offset] = (value ushr 16).toByte()
        this[offset + 1] = (value ushr 8).toByte()
        this[offset + 2] = value.toByte()
    }

    private fun frameSize(blockSize: Int, frameNumberBytes: Int, subframeBytes: Int): Int {
        val blockSizeExtraBytes = if (blockSize == BLOCK_SIZE) 0 else 2
        return 4 + frameNumberBytes + blockSizeExtraBytes + 1 + subframeBytes + 2
    }

    private const val CONSTANT_SUBFRAME_BYTES = 3 // Subframe header plus one 16-bit sample.
}
