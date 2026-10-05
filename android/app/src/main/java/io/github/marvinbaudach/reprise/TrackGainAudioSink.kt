package io.github.marvinbaudach.reprise

import androidx.media3.exoplayer.audio.AudioSink
import androidx.media3.exoplayer.audio.ForwardingAudioSink
import java.nio.ByteBuffer
import kotlin.math.pow
import kotlin.math.roundToInt

/**
 * Applies Core's per-track gain to signed 16-bit PCM, choosing the gain of the
 * item a buffer belongs to by the stream offsets Media3 announces.
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

        // The current item and the pre-fed next one; the player holds no more.
        private const val MAX_STREAMS = 2

        /** Linear factor for [gainDb]; a gain that is not finite plays at unity. */
        fun linearGain(gainDb: Double): Double =
            if (gainDb.isFinite()) {
                10.0.pow(gainDb.coerceIn(MIN_GAIN_DB, MAX_GAIN_DB) / 20.0)
            } else {
                1.0
            }
    }

    /** One media item in playlist order: its gain and, once known, its stream offset. */
    private class Stream(var gainDb: Double, var offsetUs: Long? = null)

    // The current media item first, then the pre-fed next one. Gains belong to
    // the item, never to a position in a queue, so replacing the next item,
    // seeking back across a boundary and flushing all leave the right gain.
    private val streams = ArrayList<Stream>(MAX_STREAMS)
    private var lastScaledBuffer: ByteBuffer? = null
    private var lastScaledPresentationTimeUs = Long.MIN_VALUE
    private var lastScaledLimit = -1

    @Synchronized
    fun startPlaylist(currentGainDb: Double, nextGainDb: Double?) {
        streams.clear()
        streams.add(Stream(currentGainDb))
        nextGainDb?.let { streams.add(Stream(it)) }
        clearScaledBufferMarker()
    }

    /**
     * Declares the gain of the item after the current one, or that there is none.
     *
     * A next item that was already announced keeps its offset and takes the new
     * gain: Media3 re-reads a replaced item and repeats the same offset, which
     * [setOutputStreamOffsetUs] treats as already known.
     */
    @Synchronized
    fun setNextGain(gainDb: Double?) {
        if (streams.isEmpty()) return
        when {
            gainDb == null -> while (streams.size > 1) streams.removeAt(streams.lastIndex)
            streams.size > 1 -> streams[1].gainDb = gainDb
            else -> streams.add(Stream(gainDb))
        }
    }

    /** Re-declares the gains of the current and the next item, mid-playback. */
    @Synchronized
    fun setGains(currentGainDb: Double, nextGainDb: Double?) {
        if (streams.isEmpty()) return
        streams[0].gainDb = currentGainDb
        setNextGain(nextGainDb)
    }

    /** The player moved on to the next item by itself: it is the current one now. */
    @Synchronized
    fun advanceToNext() {
        if (streams.size > 1) streams.removeAt(0)
    }

    @Synchronized
    fun clearPlaylist() {
        streams.clear()
        clearScaledBufferMarker()
    }

    override fun flush() {
        synchronized(this) { clearScaledBufferMarker() }
        super.flush()
    }

    override fun setOutputStreamOffsetUs(outputStreamOffsetUs: Long) {
        synchronized(this) { bindOffset(outputStreamOffsetUs) }
        super.setOutputStreamOffsetUs(outputStreamOffsetUs)
    }

    /** Gives [offsetUs] to the first item that has none; an offset already bound is a repeat. */
    private fun bindOffset(offsetUs: Long) {
        if (streams.isEmpty() || streams.any { it.offsetUs == offsetUs }) return
        val unbound = streams.firstOrNull { it.offsetUs == null }
        if (unbound != null) {
            unbound.offsetUs = offsetUs
        } else {
            // Every item already has an offset and this is a new one: the
            // timeline moved, so the latest announcement describes the last item.
            streams.last().offsetUs = offsetUs
        }
    }

    /** The gain of the item whose stream a buffer at [presentationTimeUs] belongs to. */
    private fun gainDbAt(presentationTimeUs: Long): Double {
        var best: Stream? = null
        for (index in streams.indices) {
            val stream = streams[index]
            val offset = stream.offsetUs ?: continue
            if (offset <= presentationTimeUs && (best == null || offset >= best.offsetUs!!)) {
                best = stream
            }
        }
        return (best ?: streams.firstOrNull())?.gainDb ?: 0.0
    }

    override fun handleBuffer(
        buffer: ByteBuffer,
        presentationTimeUs: Long,
        encodedAccessUnitCount: Int,
    ): Boolean {
        synchronized(this) {
            scaleOnce(buffer, presentationTimeUs, linearGain(gainDbAt(presentationTimeUs)))
        }
        return super.handleBuffer(buffer, presentationTimeUs, encodedAccessUnitCount)
    }

    private fun scaleOnce(buffer: ByteBuffer, presentationTimeUs: Long, gain: Double) {
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
            val scaled = (sample * gain)
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
