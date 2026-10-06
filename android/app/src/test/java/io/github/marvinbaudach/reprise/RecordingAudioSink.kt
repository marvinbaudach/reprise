package io.github.marvinbaudach.reprise

import androidx.media3.exoplayer.audio.AudioSink
import java.lang.reflect.Proxy
import java.nio.ByteBuffer
import java.nio.ByteOrder

/** Signed 16-bit little-endian PCM, ready to be read. */
internal fun pcm16(vararg samples: Int): ByteBuffer = ByteBuffer
    .allocate(samples.size * Short.SIZE_BYTES)
    .order(ByteOrder.LITTLE_ENDIAN)
    .apply {
        samples.forEach { putShort(it.toShort()) }
        flip()
    }

/** The 16-bit little-endian samples still unread in [buffer], without moving it. */
internal fun remainingSamples(buffer: ByteBuffer): List<Int> {
    val view = buffer.duplicate()
    val samples = mutableListOf<Int>()
    while (view.remaining() >= Short.SIZE_BYTES) {
        val low = view.get().toInt() and 0xff
        val high = view.get().toInt()
        samples += ((high shl 8) or low).toShort().toInt()
    }
    return samples
}

/**
 * A delegate sink that records every buffer offered to it, as the real output
 * stage would see it, and consumes up to [consumeBytesPerCall] bytes per offer.
 */
internal class RecordingAudioSink {
    class Offer(val buffer: ByteBuffer, val samples: List<Int>)

    val offers = mutableListOf<Offer>()
    var consumeBytesPerCall = Int.MAX_VALUE

    val sink: AudioSink = Proxy.newProxyInstance(
        AudioSink::class.java.classLoader,
        arrayOf(AudioSink::class.java),
    ) { _, method, arguments ->
        if (method.name == "handleBuffer") {
            val buffer = arguments!![0] as ByteBuffer
            offers += Offer(buffer, remainingSamples(buffer))
            buffer.position(buffer.position() + minOf(buffer.remaining(), consumeBytesPerCall))
            !buffer.hasRemaining()
        } else {
            when (method.returnType) {
                java.lang.Boolean.TYPE -> true
                java.lang.Integer.TYPE -> 0
                java.lang.Long.TYPE -> 0L
                java.lang.Float.TYPE -> 0f
                else -> null
            }
        }
    } as AudioSink
}
