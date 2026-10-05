package io.github.marvinbaudach.reprise

import androidx.media3.exoplayer.audio.AudioSink
import java.lang.reflect.Proxy
import java.nio.ByteBuffer
import java.nio.ByteOrder
import org.junit.Assert.assertEquals
import org.junit.Test

class TrackGainAudioSinkTest {
    private fun delegate(): AudioSink = Proxy.newProxyInstance(
        AudioSink::class.java.classLoader,
        arrayOf(AudioSink::class.java),
    ) { _, method, _ ->
        when (method.returnType) {
            java.lang.Boolean.TYPE -> true
            java.lang.Integer.TYPE -> 0
            java.lang.Long.TYPE -> 0L
            java.lang.Float.TYPE -> 0f
            else -> null
        }
    } as AudioSink

    private fun pcm16(vararg samples: Int): ByteBuffer = ByteBuffer
        .allocate(samples.size * Short.SIZE_BYTES)
        .order(ByteOrder.LITTLE_ENDIAN)
        .apply {
            samples.forEach { putShort(it.toShort()) }
            flip()
        }

    @Test
    fun gainSwitchesWhenTheFirstBufferReachesTheQueuedStreamOffset() {
        val sink = TrackGainAudioSink(delegate())
        sink.startPlaylist(-6.020599913, 6.020599913)
        sink.setOutputStreamOffsetUs(0)
        val first = pcm16(10_000, -10_000)

        sink.handleBuffer(first, 0, 1)

        assertEquals(5_000, first.getShort(0).toInt())
        assertEquals(-5_000, first.getShort(2).toInt())

        sink.setOutputStreamOffsetUs(1_000_000)
        val beforeBoundary = pcm16(10_000)
        sink.handleBuffer(beforeBoundary, 999_999, 1)
        assertEquals(5_000, beforeBoundary.getShort(0).toInt())

        val atBoundary = pcm16(10_000)
        sink.handleBuffer(atBoundary, 1_000_000, 1)
        assertEquals(20_000, atBoundary.getShort(0).toInt())
    }

    @Test
    fun positiveGainSaturatesSignedPcm16InsteadOfWrapping() {
        val sink = TrackGainAudioSink(delegate())
        sink.startPlaylist(12.0, null)
        sink.setOutputStreamOffsetUs(0)
        val buffer = pcm16(20_000, -20_000)

        sink.handleBuffer(buffer, 0, 1)

        assertEquals(Short.MAX_VALUE.toInt(), buffer.getShort(0).toInt())
        assertEquals(Short.MIN_VALUE.toInt(), buffer.getShort(2).toInt())
    }

    @Test
    fun aRepeatedOffsetForTheSameStreamKeepsTheQueuedNextGain() {
        val sink = TrackGainAudioSink(delegate())
        sink.startPlaylist(0.0, 6.020599913)
        sink.setOutputStreamOffsetUs(0)
        sink.setOutputStreamOffsetUs(0)

        sink.setOutputStreamOffsetUs(1_000_000)
        val next = pcm16(10_000)
        sink.handleBuffer(next, 1_000_000, 1)

        assertEquals(20_000, next.getShort(0).toInt())
    }
}
