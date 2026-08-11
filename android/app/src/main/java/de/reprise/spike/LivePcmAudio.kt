package de.reprise.spike

import android.content.Context
import androidx.media3.common.C
import androidx.media3.common.TrackSelectionParameters
import androidx.media3.common.audio.AudioProcessor
import androidx.media3.exoplayer.DefaultRenderersFactory
import androidx.media3.exoplayer.audio.AudioSink
import androidx.media3.exoplayer.audio.DefaultAudioSink
import androidx.media3.exoplayer.audio.TeeAudioProcessor
import java.nio.ByteBuffer

/** A borrowed PCM buffer is valid only for the duration of [ingestPcm16]. */
internal interface LivePcmConsumer {
    fun ingestPcm16(
        bytes: ByteArray,
        byteCount: Int,
        sampleRateHz: Int,
        channelCount: Int,
    )

    fun resetAudioStream()
}

/**
 * Copies decoded PCM into one reusable byte array and leaves Media3's own
 * output path to [TeeAudioProcessor]. No work is done while no visualizer is
 * attached.
 */
internal class LivePcmBufferSink : TeeAudioProcessor.AudioBufferSink {
    @Volatile
    private var consumer: LivePcmConsumer? = null
    private var sampleRateHz = 0
    private var channelCount = 0
    private var encoding = C.ENCODING_INVALID
    private var scratch = ByteArray(0)

    fun attach(consumer: LivePcmConsumer) {
        this.consumer = consumer
    }

    fun detach(consumer: LivePcmConsumer) {
        if (this.consumer === consumer) {
            this.consumer = null
            resetSafely(consumer)
        }
    }

    fun detachAll() {
        val detached = consumer
        consumer = null
        detached?.let(::resetSafely)
    }

    override fun flush(sampleRateHz: Int, channelCount: Int, encoding: Int) {
        this.sampleRateHz = sampleRateHz
        this.channelCount = channelCount
        this.encoding = encoding
        consumer?.let(::resetSafely)
    }

    override fun handleBuffer(buffer: ByteBuffer) {
        val target = consumer ?: return
        if (encoding != C.ENCODING_PCM_16BIT || sampleRateHz <= 0 || channelCount <= 0) return
        try {
            val byteCount = buffer.remaining()
            if (scratch.size < byteCount) scratch = ByteArray(byteCount)
            buffer.get(scratch, 0, byteCount)
            target.ingestPcm16(scratch, byteCount, sampleRateHz, channelCount)
        } catch (_: Throwable) {
            // The visualizer is optional: discard this frame, never the audio.
        }
    }

    private fun resetSafely(target: LivePcmConsumer) {
        try {
            target.resetAudioStream()
        } catch (_: Throwable) {
            // The visualizer is optional: discard its history, never the audio.
        }
    }
}

/** Installs the tee after Media3's built-in conversion to signed 16-bit PCM. */
internal class LivePcmRenderersFactory(
    context: Context,
    private val processor: AudioProcessor,
) : DefaultRenderersFactory(context) {
    public override fun buildAudioSink(
        context: Context,
        enableFloatOutput: Boolean,
        enableAudioOutputPlaybackParameters: Boolean,
    ): AudioSink {
        require(!enableFloatOutput) {
            "Live PCM visualization requires signed 16-bit audio output"
        }
        return DefaultAudioSink.Builder(context)
            // Media3 prepends caller processors before SilenceSkipping and Sonic,
            // so this tap observes audio before silence removal or speed changes.
            .setAudioProcessors(arrayOf(processor))
            .setEnableFloatOutput(false)
            .setEnableAudioOutputPlaybackParameters(enableAudioOutputPlaybackParameters)
            .build()
    }
}

/**
 * Media3 1.10.1 defaults offload to disabled; keep that contract explicit so
 * the decoded-PCM processor cannot silently be bypassed by a future default.
 */
internal fun livePcmAudioOffloadPreferences():
    TrackSelectionParameters.AudioOffloadPreferences =
    TrackSelectionParameters.AudioOffloadPreferences.Builder()
        .setAudioOffloadMode(
            TrackSelectionParameters.AudioOffloadPreferences.AUDIO_OFFLOAD_MODE_DISABLED,
        )
        .build()
