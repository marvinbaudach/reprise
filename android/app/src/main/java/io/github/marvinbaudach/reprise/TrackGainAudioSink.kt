package io.github.marvinbaudach.reprise

import androidx.media3.exoplayer.audio.AudioSink
import androidx.media3.exoplayer.audio.ForwardingAudioSink
import java.nio.ByteBuffer
import java.nio.ByteOrder
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

        private const val UNITY_GAIN = 1.0

        // The current item and the pre-fed next one; the player holds no more.
        private const val MAX_STREAMS = 2

        // Room for a few tens of milliseconds of stereo audio; it grows on demand.
        private const val INITIAL_SCRATCH_BYTES = 16 * 1024

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

    // The scaled copy that is forwarded, and the input it was made from. A
    // buffer the output stage only partly takes is offered again by the
    // renderer, as the same object at the same time: that retry is forwarded
    // from the copy as it stands, not scaled a second time.
    private var scratch: ByteBuffer =
        ByteBuffer.allocateDirect(INITIAL_SCRATCH_BYTES).order(ByteOrder.LITTLE_ENDIAN)
    private var pendingInput: ByteBuffer? = null
    private var pendingPresentationTimeUs = Long.MIN_VALUE
    private var pendingInputStart = 0
    private var pendingBypass = false

    @Synchronized
    fun startPlaylist(currentGainDb: Double, nextGainDb: Double?) {
        streams.clear()
        streams.add(Stream(currentGainDb))
        nextGainDb?.let { streams.add(Stream(it)) }
        clearPendingBuffer()
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
        clearPendingBuffer()
    }

    override fun reset() {
        synchronized(this) { clearPendingBuffer() }
        super.reset()
    }

    override fun flush() {
        synchronized(this) { clearPendingBuffer() }
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
        val forwarded = synchronized(this) {
            val isRetry = pendingInput === buffer && pendingPresentationTimeUs == presentationTimeUs
            if (!isRetry) beginOffer(buffer, presentationTimeUs)
            if (pendingBypass) buffer else scratch
        }
        val consumedAll = try {
            super.handleBuffer(forwarded, presentationTimeUs, encodedAccessUnitCount)
        } catch (failure: Throwable) {
            synchronized(this) { clearPendingBuffer() }
            throw failure
        }
        synchronized(this) {
            if (consumedAll) {
                if (forwarded !== buffer) buffer.position(buffer.limit())
                clearPendingBuffer()
            } else if (forwarded !== buffer) {
                // The delegate took only part of the copy; the renderer offers
                // the same input again and expects it to show what is left.
                buffer.position(pendingInputStart + forwarded.position())
            }
        }
        return consumedAll
    }

    /**
     * Starts forwarding [input]: at exactly unity gain the input itself goes
     * through untouched; otherwise its scaled samples are written into the
     * sink's own buffer. Either way the choice is remembered with the input it
     * was made for, so a retry is forwarded as it is, whatever the gain became.
     *
     * The input is only read: it is Media3's codec output, which can be
     * read-only, and nothing downstream should see it change under it.
     */
    private fun beginOffer(input: ByteBuffer, presentationTimeUs: Long) {
        val gain = linearGain(gainDbAt(presentationTimeUs))
        pendingInput = input
        pendingPresentationTimeUs = presentationTimeUs
        pendingInputStart = input.position()
        pendingBypass = gain == UNITY_GAIN
        if (!pendingBypass) scaleIntoScratch(input, gain)
    }

    private fun scaleIntoScratch(input: ByteBuffer, gain: Double) {
        val start = input.position()
        val length = input.remaining()
        if (scratch.capacity() < length) {
            scratch = ByteBuffer.allocateDirect(maxOf(length, scratch.capacity() * 2))
                .order(ByteOrder.LITTLE_ENDIAN)
        }
        scratch.clear()
        var offset = 0
        while (offset + 1 < length) {
            val low = input.get(start + offset).toInt() and 0xff
            val high = input.get(start + offset + 1).toInt()
            val sample = ((high shl 8) or low).toShort().toInt()
            val scaled = (sample * gain)
                .roundToInt()
                .coerceIn(Short.MIN_VALUE.toInt(), Short.MAX_VALUE.toInt())
            scratch.putShort(offset, scaled.toShort())
            offset += Short.SIZE_BYTES
        }
        if (offset < length) scratch.put(offset, input.get(start + offset))
        scratch.limit(length).position(0)
    }

    private fun clearPendingBuffer() {
        pendingInput = null
        pendingPresentationTimeUs = Long.MIN_VALUE
        pendingInputStart = 0
        pendingBypass = false
    }
}
