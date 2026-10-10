package io.github.marvinbaudach.reprise

import androidx.media3.common.C
import androidx.media3.common.util.UnstableApi
import androidx.media3.exoplayer.audio.AudioSink
import androidx.media3.exoplayer.audio.ForwardingAudioSink
import java.nio.ByteBuffer
import java.nio.ByteOrder
import kotlin.math.pow
import kotlin.math.roundToInt

/**
 * Applies Core's per-track gain to signed 16-bit PCM, choosing the gain of the
 * stream Media3 announces by announcement order. The announcement itself marks
 * the boundary before the new stream's first buffer; the later media-item
 * transition only retires the previous item from the bookkeeping.
 *
 * Media3 hands over a clip of a CUE file at the first decoded buffer that
 * starts before the cut, and that buffer runs past it (a FLAC frame is up to
 * 93 ms, an Opus packet 20 ms) with the next track's first samples. When the
 * next item continues the same file at a known position, the sink therefore
 * switches the gain inside that one buffer, at the sample the cut falls on.
 *
 * Where a new decoder starts for the next stream, Media3 offers that stream's
 * first buffer before it announces the stream; the configuration for the next
 * period, which precedes the buffer, starts the next stream there.
 *
 * `LivePcmRenderersFactory` refuses float output and keeps offload off, so every
 * buffer this sink sees is 16-bit PCM; `configure` is final in the base class,
 * which is why the width is a contract of the factory rather than checked here.
 */
@androidx.annotation.OptIn(UnstableApi::class)
internal class TrackGainAudioSink(delegate: AudioSink) : ForwardingAudioSink(delegate) {
    internal companion object {
        // The same bounds Core resolves into, re-checked here because this value
        // reaches the audio thread: a bad one must not blast or mute playback.
        const val MIN_GAIN_DB = -24.0
        const val MAX_GAIN_DB = 12.0

        private const val UNITY_GAIN = 1.0
        private const val MICROS_PER_SECOND = 1_000_000L

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

    /**
     * One media item in playlist order. [startsAtUs] is the position in its
     * file at which the item continues the one before it without a gap, when
     * it does; the position is in the same timeline as the file's own
     * timestamps, which Media3 shifts by the announced stream offset.
     */
    private class Stream(var gainDb: Double, var startsAtUs: Long? = null)

    // The current media item first, then the pre-fed next one. Gains belong to
    // the item, never to a position in a queue, so replacing the next item,
    // seeking back across a boundary and flushing all leave the right gain.
    private val streams = ArrayList<Stream>(MAX_STREAMS)
    private var activeIndex = 0
    private var wroteSinceStreamStart = false

    // The offset Media3 announced for the stream that is being written, and the
    // shape of the audio it is configured with: together they turn a position in
    // the file into a sample index inside a buffer. Neither is cleared by a
    // flush, which keeps the stream and its format.
    private var outputStreamOffsetUs = C.TIME_UNSET
    private var writtenPeriodUid: Any? = null
    private var sampleRateHz = 0
    private var bytesPerFrame = 0

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
    fun startPlaylist(currentGainDb: Double, nextGainDb: Double?, nextStartsAtUs: Long? = null) {
        streams.clear()
        streams.add(Stream(currentGainDb))
        nextGainDb?.let { streams.add(Stream(it, nextStartsAtUs)) }
        resetStreamPosition()
        clearPendingBuffer()
    }

    /**
     * Declares the gain of the item after the current one, or that there is none.
     *
     * A next item that was already announced stays active and takes the new
     * gain in place. [startsAtUs] is where in its file the next item continues
     * the current one, or null when it does not.
     */
    @Synchronized
    fun setNextGain(gainDb: Double?, startsAtUs: Long? = null) {
        if (streams.isEmpty()) return
        when {
            gainDb == null -> {
                while (streams.size > 1) streams.removeAt(streams.lastIndex)
                activeIndex = minOf(activeIndex, streams.lastIndex)
            }
            streams.size > 1 -> streams[1].apply {
                this.gainDb = gainDb
                this.startsAtUs = startsAtUs
            }
            else -> streams.add(Stream(gainDb, startsAtUs))
        }
    }

    /** Re-declares the gains of the current and the next item, mid-playback. */
    @Synchronized
    fun setGains(currentGainDb: Double, nextGainDb: Double?) {
        if (streams.isEmpty()) return
        streams[0].gainDb = currentGainDb
        // Only the gain moves: where the next item continues the current one
        // does not change with a settings change.
        setNextGain(nextGainDb, streams.getOrNull(1)?.startsAtUs)
    }

    /** The player moved on to the next item by itself: it is the current one now. */
    @Synchronized
    fun advanceToNext() {
        if (streams.size > 1) streams.removeAt(0)
        activeIndex = maxOf(0, activeIndex - 1)
    }

    @Synchronized
    fun clearPlaylist() {
        streams.clear()
        resetStreamPosition()
        clearPendingBuffer()
    }

    override fun reset() {
        synchronized(this) {
            resetStreamPosition()
            clearPendingBuffer()
            // A reset drops the configuration: the next stream configures the
            // sink anew. A flush keeps it, and with it the period being written.
            writtenPeriodUid = null
        }
        super.reset()
    }

    override fun flush() {
        synchronized(this) {
            resetStreamPosition()
            clearPendingBuffer()
        }
        super.flush()
    }

    override fun configure(config: AudioSink.AudioSinkConfig) {
        synchronized(this) {
            val format = config.format
            sampleRateHz = format.sampleRate.coerceAtLeast(0)
            bytesPerFrame = format.channelCount.coerceAtLeast(0) * Short.SIZE_BYTES
            // Where a new decoder starts for the next stream, Media3 configures the
            // sink for it and offers its first buffer before it announces the
            // stream offset, so the announcement alone would give that buffer
            // the previous track's gain. A configuration for another period is the
            // earlier signal that the next stream has begun.
            val periodUid = config.mediaPeriodId?.periodUid
            if (periodUid != null) {
                if (writtenPeriodUid != null && writtenPeriodUid != periodUid) startNextStream()
                writtenPeriodUid = periodUid
            }
        }
        super.configure(config)
    }

    override fun setOutputStreamOffsetUs(outputStreamOffsetUs: Long) {
        synchronized(this) {
            this.outputStreamOffsetUs = outputStreamOffsetUs
            startNextStream()
        }
        super.setOutputStreamOffsetUs(outputStreamOffsetUs)
    }

    /**
     * The next stream has begun, by whichever signal reached the sink first;
     * the other one then finds nothing written since and changes nothing.
     */
    private fun startNextStream() {
        if (streams.isNotEmpty() && wroteSinceStreamStart) {
            wroteSinceStreamStart = false
            if (activeIndex + 1 < streams.size) activeIndex += 1
        }
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
        val gain = linearGain(streams.getOrNull(activeIndex)?.gainDb ?: 0.0)
        val splitAt = nextStreamStartByte(input, presentationTimeUs)
        val gainAfterSplit = if (splitAt < input.remaining()) {
            linearGain(streams.getOrNull(activeIndex + 1)?.gainDb ?: 0.0)
        } else {
            gain
        }
        wroteSinceStreamStart = true
        pendingInput = input
        pendingPresentationTimeUs = presentationTimeUs
        pendingInputStart = input.position()
        pendingBypass = gain == UNITY_GAIN && gainAfterSplit == UNITY_GAIN
        if (!pendingBypass) scaleIntoScratch(input, gain, gainAfterSplit, splitAt)
    }

    /**
     * The byte of [input] at which the next stream's audio begins, or its length
     * when the buffer holds none of it: the next item continues this file at a
     * known position, and that position falls inside the buffer.
     */
    private fun nextStreamStartByte(input: ByteBuffer, presentationTimeUs: Long): Int {
        val length = input.remaining()
        val startsAtUs = streams.getOrNull(activeIndex + 1)?.startsAtUs ?: return length
        if (outputStreamOffsetUs == C.TIME_UNSET || sampleRateHz <= 0 || bytesPerFrame <= 0) {
            return length
        }
        val microsIntoBuffer = startsAtUs + outputStreamOffsetUs - presentationTimeUs
        val frame = Math.round(microsIntoBuffer.toDouble() * sampleRateHz / MICROS_PER_SECOND)
        val frames = length / bytesPerFrame
        return frame.coerceIn(0L, frames.toLong()).toInt() * bytesPerFrame
    }

    private fun scaleIntoScratch(input: ByteBuffer, gain: Double, gainAfterSplit: Double, splitAt: Int) {
        val start = input.position()
        val length = input.remaining()
        if (scratch.capacity() < length) {
            scratch = ByteBuffer.allocateDirect(maxOf(length, scratch.capacity() * 2))
                .order(ByteOrder.LITTLE_ENDIAN)
        }
        scratch.clear()
        var offset = 0
        var factor = gain
        while (offset + 1 < length) {
            if (offset == splitAt) factor = gainAfterSplit
            val low = input.get(start + offset).toInt() and 0xff
            val high = input.get(start + offset + 1).toInt()
            val sample = ((high shl 8) or low).toShort().toInt()
            val scaled = (sample * factor)
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

    private fun resetStreamPosition() {
        activeIndex = 0
        wroteSinceStreamStart = false
    }
}
