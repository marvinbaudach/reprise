package io.github.marvinbaudach.reprise

import java.io.File
import java.nio.ByteBuffer
import java.nio.ByteOrder

/** Small signed 16-bit PCM WAV fixtures whose every frame is one chosen constant. */
internal object TestWavFile {
    const val SAMPLE_RATE = 8_000
    const val CHANNELS = 2
    private const val BYTES_PER_SAMPLE = 2
    private const val HEADER_BYTES = 44

    /** Frames per millisecond at [SAMPLE_RATE]. */
    const val FRAMES_PER_MS = SAMPLE_RATE / 1_000

    /**
     * Writes a stereo file in which frames `[0, firstLengthMs)` all hold [firstValue]
     * and the following [secondLengthMs] all hold [secondValue], so a sample says by
     * its value which stretch of the file it came from.
     */
    fun writeTwoStretches(
        file: File,
        firstValue: Int,
        firstLengthMs: Int,
        secondValue: Int,
        secondLengthMs: Int,
    ): File {
        val frames = (firstLengthMs + secondLengthMs) * FRAMES_PER_MS
        val dataBytes = frames * CHANNELS * BYTES_PER_SAMPLE
        val out = ByteBuffer.allocate(HEADER_BYTES + dataBytes).order(ByteOrder.LITTLE_ENDIAN)
        out.put("RIFF".toByteArray()).putInt(36 + dataBytes)
        out.put("WAVE".toByteArray()).put("fmt ".toByteArray()).putInt(16)
        out.putShort(1).putShort(CHANNELS.toShort()).putInt(SAMPLE_RATE)
        out.putInt(SAMPLE_RATE * CHANNELS * BYTES_PER_SAMPLE)
        out.putShort((CHANNELS * BYTES_PER_SAMPLE).toShort()).putShort(16)
        out.put("data".toByteArray()).putInt(dataBytes)
        repeat(frames) { frame ->
            val value = if (frame < firstLengthMs * FRAMES_PER_MS) firstValue else secondValue
            repeat(CHANNELS) { out.putShort(value.toShort()) }
        }
        file.writeBytes(out.array())
        return file
    }
}
