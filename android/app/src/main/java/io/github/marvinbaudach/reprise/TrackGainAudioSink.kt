package io.github.marvinbaudach.reprise

import androidx.media3.exoplayer.audio.AudioSink
import androidx.media3.exoplayer.audio.ForwardingAudioSink
import java.nio.ByteBuffer
import java.util.ArrayDeque
import kotlin.math.pow
import kotlin.math.roundToInt

/**
 * Applies Core's per-track gain to signed 16-bit PCM at Media3 stream offsets.
 *
 * `LivePcmRenderersFactory` refuses float output and keeps offload off, so every
 * buffer this sink sees is 16-bit PCM; `configure` is final in the base class,
 * which is why the width is a contract of the factory rather than checked here.
 */
internal class TrackGainAudioSink(delegate: AudioSink) : ForwardingAudioSink(delegate) {
    internal companion object {
        // The same bounds Core resolves into, re-checked here because this value
        // reaches the audio thread: a bad one must not blast or mute playback.
        const val MIN_GAIN_DB = -24.0
        const val MAX_GAIN_DB = 12.0

        /** Linear factor for [gainDb]; a gain that is not finite plays at unity. */
        fun linearGain(gainDb: Double): Double =
            if (gainDb.isFinite()) {
                10.0.pow(gainDb.coerceIn(MIN_GAIN_DB, MAX_GAIN_DB) / 20.0)
            } else {
                1.0
            }
    }

    private data class Boundary(val offsetUs: Long, val gainDb: Double)

    private val queuedGains = ArrayDeque<Double>()
    private val boundaries = ArrayDeque<Boundary>()
    private var currentLinearGain = 1.0
    private var awaitingCurrentOffset = false
    private var lastOffsetUs: Long? = null
    private var lastScaledBuffer: ByteBuffer? = null
    private var lastScaledPresentationTimeUs = Long.MIN_VALUE
    private var lastScaledLimit = -1

    @Synchronized
    fun startPlaylist(currentGainDb: Double, nextGainDb: Double?) {
        queuedGains.clear()
        boundaries.clear()
        queuedGains.addLast(currentGainDb)
        nextGainDb?.let(queuedGains::addLast)
        currentLinearGain = 1.0
        awaitingCurrentOffset = true
        lastOffsetUs = null
        clearScaledBufferMarker()
    }

    @Synchronized
    fun setNextGain(gainDb: Double?) {
        val current = if (awaitingCurrentOffset && queuedGains.isNotEmpty()) {
            queuedGains.removeFirst()
        } else {
            null
        }
        queuedGains.clear()
        current?.let(queuedGains::addLast)
        gainDb?.let(queuedGains::addLast)
    }

    @Synchronized
    fun clearPlaylist() {
        queuedGains.clear()
        boundaries.clear()
        currentLinearGain = 1.0
        awaitingCurrentOffset = false
        lastOffsetUs = null
        clearScaledBufferMarker()
    }

    override fun flush() {
        synchronized(this) { clearScaledBufferMarker() }
        super.flush()
    }

    override fun setOutputStreamOffsetUs(outputStreamOffsetUs: Long) {
        synchronized(this) {
            // Media3 may repeat the offset of the stream it is already playing
            // (a seek resets the codec); that must not consume the next gain.
            if (lastOffsetUs == outputStreamOffsetUs) return@synchronized
            lastOffsetUs = outputStreamOffsetUs
            val gainDb = if (queuedGains.isEmpty()) 0.0 else queuedGains.removeFirst()
            boundaries.addLast(Boundary(outputStreamOffsetUs, gainDb))
            if (awaitingCurrentOffset) awaitingCurrentOffset = false
        }
        super.setOutputStreamOffsetUs(outputStreamOffsetUs)
    }

    override fun handleBuffer(
        buffer: ByteBuffer,
        presentationTimeUs: Long,
        encodedAccessUnitCount: Int,
    ): Boolean {
        synchronized(this) {
            while (boundaries.isNotEmpty() && presentationTimeUs >= boundaries.peekFirst().offsetUs) {
                currentLinearGain = linearGain(boundaries.removeFirst().gainDb)
                clearScaledBufferMarker()
            }
            scaleOnce(buffer, presentationTimeUs)
        }
        return super.handleBuffer(buffer, presentationTimeUs, encodedAccessUnitCount)
    }

    private fun scaleOnce(buffer: ByteBuffer, presentationTimeUs: Long) {
        if (
            lastScaledBuffer === buffer &&
            lastScaledPresentationTimeUs == presentationTimeUs &&
            lastScaledLimit == buffer.limit()
        ) {
            return
        }
        var index = buffer.position()
        while (index + 1 < buffer.limit()) {
            val low = buffer.get(index).toInt() and 0xff
            val high = buffer.get(index + 1).toInt()
            val sample = ((high shl 8) or low).toShort().toInt()
            val scaled = (sample * currentLinearGain)
                .roundToInt()
                .coerceIn(Short.MIN_VALUE.toInt(), Short.MAX_VALUE.toInt())
            buffer.put(index, (scaled and 0xff).toByte())
            buffer.put(index + 1, ((scaled ushr 8) and 0xff).toByte())
            index += Short.SIZE_BYTES
        }
        lastScaledBuffer = buffer
        lastScaledPresentationTimeUs = presentationTimeUs
        lastScaledLimit = buffer.limit()
    }

    private fun clearScaledBufferMarker() {
        lastScaledBuffer = null
        lastScaledPresentationTimeUs = Long.MIN_VALUE
        lastScaledLimit = -1
    }
}
