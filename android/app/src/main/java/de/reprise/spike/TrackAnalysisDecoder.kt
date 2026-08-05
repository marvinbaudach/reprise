package de.reprise.spike

import android.content.Context
import android.media.AudioFormat
import android.media.MediaCodec
import android.media.MediaExtractor
import android.media.MediaFormat
import android.net.Uri
import java.nio.ByteBuffer
import java.nio.ByteOrder

private const val CODEC_TIMEOUT_US = 10_000L
private const val AUDIO_MIME_PREFIX = "audio/"

internal class UnsupportedPcmFormatException(encoding: Int) :
    IllegalArgumentException("MediaCodec produced unsupported PCM encoding $encoding")

/** Converts one codec output buffer without retaining it or a whole-track copy. */
internal object PcmOutputPump {
    /**
     * Android decoders normally expose signed 16-bit PCM and may expose float
     * PCM; both are converted here to interleaved f32. Any other encoding fails
     * the pass, whose owner cancels the analysis session rather than guessing.
     */
    fun push(
        buffer: ByteBuffer,
        pcmEncoding: Int,
        sampleRateHz: Int,
        channelCount: Int,
        sink: (List<Float>, UInt, UInt) -> Unit,
    ) {
        require(sampleRateHz > 0) { "MediaCodec returned a non-positive sample rate" }
        require(channelCount > 0) { "MediaCodec returned a non-positive channel count" }
        val bytesPerSample = when (pcmEncoding) {
            AudioFormat.ENCODING_PCM_16BIT -> Short.SIZE_BYTES
            AudioFormat.ENCODING_PCM_FLOAT -> Float.SIZE_BYTES
            else -> throw UnsupportedPcmFormatException(pcmEncoding)
        }
        require(buffer.remaining() % bytesPerSample == 0) {
            "MediaCodec returned a partial PCM sample"
        }
        require((buffer.remaining() / bytesPerSample) % channelCount == 0) {
            "MediaCodec returned a partial interleaved PCM frame"
        }
        val maximumSamples = Math.multiplyExact(sampleRateHz, channelCount)
        val pcm = buffer.slice().order(ByteOrder.nativeOrder())
        while (pcm.hasRemaining()) {
            val sampleCount = minOf(pcm.remaining() / bytesPerSample, maximumSamples)
            val samples = FloatArray(sampleCount)
            for (index in samples.indices) {
                samples[index] = when (pcmEncoding) {
                    AudioFormat.ENCODING_PCM_16BIT -> pcm.short / 32768f
                    AudioFormat.ENCODING_PCM_FLOAT -> pcm.float
                    else -> error("encoding was validated above")
                }
            }
            sink(samples.asList(), sampleRateHz.toUInt(), channelCount.toUInt())
        }
    }
}

/** Decodes a content URI as fast as its codec can drain, with no playback clock or AudioTrack. */
internal class AndroidMediaCodecTrackDecoder(
    private val context: Context,
) : TrackPcmDecoder {
    override fun decode(contentUri: String, sink: PcmSink, cancelled: () -> Boolean) {
        val extractor = MediaExtractor()
        try {
            extractor.setDataSource(context, Uri.parse(contentUri), null)
            val inputFormat = selectAudioTrack(extractor)
            val mime = requireNotNull(inputFormat.getString(MediaFormat.KEY_MIME)) {
                "audio track has no MIME type"
            }
            val codec = MediaCodec.createDecoderByType(mime)
            try {
                codec.configure(inputFormat, null, null, 0)
                codec.start()
                drain(extractor, codec, inputFormat, sink, cancelled)
            } finally {
                runCatching(codec::stop)
                codec.release()
            }
        } finally {
            extractor.release()
        }
    }

    private fun selectAudioTrack(extractor: MediaExtractor): MediaFormat {
        for (index in 0 until extractor.trackCount) {
            val format = extractor.getTrackFormat(index)
            if (format.getString(MediaFormat.KEY_MIME)?.startsWith(AUDIO_MIME_PREFIX) == true) {
                extractor.selectTrack(index)
                return format
            }
        }
        throw IllegalArgumentException("content URI contains no decodable audio track")
    }

    private fun drain(
        extractor: MediaExtractor,
        codec: MediaCodec,
        inputFormat: MediaFormat,
        sink: PcmSink,
        cancelled: () -> Boolean,
    ) {
        val bufferInfo = MediaCodec.BufferInfo()
        var inputEnded = false
        var outputEnded = false
        var outputFormat: MediaFormat? = null
        while (!outputEnded && !cancelled()) {
            if (!inputEnded) {
                val inputIndex = codec.dequeueInputBuffer(CODEC_TIMEOUT_US)
                if (inputIndex >= 0) {
                    val input = requireNotNull(codec.getInputBuffer(inputIndex))
                    input.clear()
                    val size = extractor.readSampleData(input, 0)
                    if (size < 0) {
                        codec.queueInputBuffer(
                            inputIndex,
                            0,
                            0,
                            0,
                            MediaCodec.BUFFER_FLAG_END_OF_STREAM,
                        )
                        inputEnded = true
                    } else {
                        codec.queueInputBuffer(
                            inputIndex,
                            0,
                            size,
                            extractor.sampleTime.coerceAtLeast(0L),
                            0,
                        )
                        extractor.advance()
                    }
                }
            }

            when (val outputIndex = codec.dequeueOutputBuffer(bufferInfo, CODEC_TIMEOUT_US)) {
                MediaCodec.INFO_OUTPUT_FORMAT_CHANGED -> outputFormat = codec.outputFormat
                MediaCodec.INFO_TRY_AGAIN_LATER -> Unit
                else -> if (outputIndex >= 0) {
                    try {
                        if (
                            bufferInfo.size > 0 &&
                            bufferInfo.flags and MediaCodec.BUFFER_FLAG_CODEC_CONFIG == 0
                        ) {
                            val output = requireNotNull(codec.getOutputBuffer(outputIndex)).duplicate()
                            output.position(bufferInfo.offset)
                            output.limit(bufferInfo.offset + bufferInfo.size)
                            pumpOutput(output.slice(), outputFormat ?: codec.outputFormat, inputFormat, sink)
                        }
                        outputEnded = bufferInfo.flags and MediaCodec.BUFFER_FLAG_END_OF_STREAM != 0
                    } finally {
                        codec.releaseOutputBuffer(outputIndex, false)
                    }
                }
            }
        }
    }

    private fun pumpOutput(
        output: ByteBuffer,
        outputFormat: MediaFormat,
        inputFormat: MediaFormat,
        sink: PcmSink,
    ) {
        val sampleRate = formatInteger(outputFormat, MediaFormat.KEY_SAMPLE_RATE)
            ?: formatInteger(inputFormat, MediaFormat.KEY_SAMPLE_RATE)
            ?: error("MediaCodec output has no sample rate")
        val channelCount = formatInteger(outputFormat, MediaFormat.KEY_CHANNEL_COUNT)
            ?: formatInteger(inputFormat, MediaFormat.KEY_CHANNEL_COUNT)
            ?: error("MediaCodec output has no channel count")
        // Android specifies 16-bit PCM as the raw-decoder default when the key
        // is absent. Explicit float is the other supported output; 8/24/32-bit
        // integer output fails the pass and its session is cancelled.
        val encoding = formatInteger(outputFormat, MediaFormat.KEY_PCM_ENCODING)
            ?: AudioFormat.ENCODING_PCM_16BIT
        PcmOutputPump.push(output, encoding, sampleRate, channelCount, sink)
    }
}

private fun formatInteger(format: MediaFormat, key: String): Int? =
    if (format.containsKey(key)) format.getInteger(key) else null
