package io.github.marvinbaudach.reprise

import android.content.ContentResolver
import android.media.MediaCodec
import android.media.MediaExtractor
import android.media.MediaFormat
import android.net.Uri
import android.os.Process
import uniffi.reprise_android_ffi.AnalysisDecodeException
import uniffi.reprise_android_ffi.AnalysisPcmSink
import uniffi.reprise_android_ffi.TrackPcmDecoder

/**
 * Decodes one track to 16-bit PCM with the platform's `MediaExtractor` and
 * `MediaCodec` in synchronous mode, and pushes it into the Rust-owned
 * [AnalysisPcmSink]. All the maths — downmix, resample, spectrogram and
 * waveform — stays in Rust (decision 2 of
 * `docs/plans/the-phone-analyses-its-own-music.md`); this class only pumps
 * PCM. Not Robolectric-testable: the decode loop needs a real codec, so this
 * is verified on the device.
 */
internal class MediaCodecTrackDecoder(
    private val contentResolver: ContentResolver,
) : TrackPcmDecoder {
    override fun decode(trackUri: String, sink: AnalysisPcmSink, background: Boolean) {
        val previousPriority = Process.getThreadPriority(Process.myTid())
        if (background) {
            Process.setThreadPriority(Process.THREAD_PRIORITY_BACKGROUND)
        }
        var extractor: MediaExtractor? = null
        var codec: MediaCodec? = null
        try {
            val descriptor = contentResolver.openFileDescriptor(Uri.parse(trackUri), "r")
                ?: throw AnalysisDecodeException.DecodeFailed(
                    "no file descriptor for $trackUri",
                )
            descriptor.use { parcelFileDescriptor ->
                val mediaExtractor = MediaExtractor().also { extractor = it }
                mediaExtractor.setDataSource(parcelFileDescriptor.fileDescriptor)
                val trackIndex = (0 until mediaExtractor.trackCount).firstOrNull { index ->
                    mediaExtractor.getTrackFormat(index)
                        .getString(MediaFormat.KEY_MIME)
                        ?.startsWith("audio/") == true
                } ?: throw AnalysisDecodeException.DecodeFailed(
                    "no audio track in $trackUri",
                )
                mediaExtractor.selectTrack(trackIndex)
                val inputFormat = mediaExtractor.getTrackFormat(trackIndex)
                val mime = inputFormat.getString(MediaFormat.KEY_MIME)
                    ?: throw AnalysisDecodeException.DecodeFailed(
                        "no MIME type for $trackUri",
                    )
                val mediaCodec = MediaCodec.createDecoderByType(mime).also { codec = it }
                mediaCodec.configure(inputFormat, null, null, 0)
                mediaCodec.start()
                runDecodeLoop(mediaExtractor, mediaCodec, inputFormat, sink)
            }
        } catch (decodeError: AnalysisDecodeException) {
            throw decodeError
        } catch (error: Exception) {
            throw AnalysisDecodeException.DecodeFailed(
                error.message ?: error.javaClass.simpleName,
            )
        } finally {
            codec?.let {
                it.stop()
                it.release()
            }
            extractor?.release()
            if (background) {
                Process.setThreadPriority(previousPriority)
            }
        }
    }

    /**
     * Feeds compressed samples in and pushes decoded 16-bit PCM chunks into
     * [sink] until end of stream or until [AnalysisPcmSink.pushPcmI16]
     * returns `false` (cancelled, or the session refused the chunk).
     */
    private fun runDecodeLoop(
        extractor: MediaExtractor,
        codec: MediaCodec,
        inputFormat: MediaFormat,
        sink: AnalysisPcmSink,
    ) {
        val bufferInfo = MediaCodec.BufferInfo()
        var sawInputEnd = false
        var sawOutputEnd = false
        // The codec's own output format is not queryable until
        // INFO_OUTPUT_FORMAT_CHANGED fires; the input format is the best
        // guess until then, and every real Android decoder delivers that
        // event before its first data buffer.
        var sampleRateHz = inputFormat.sampleRateOrDefault()
        var channelCount = inputFormat.channelCountOrDefault()

        while (!sawOutputEnd) {
            if (!sawInputEnd) {
                val inputIndex = codec.dequeueInputBuffer(TIMEOUT_US)
                if (inputIndex >= 0) {
                    val inputBuffer = codec.getInputBuffer(inputIndex)
                        ?: throw AnalysisDecodeException.DecodeFailed("no input buffer")
                    val sampleSize = extractor.readSampleData(inputBuffer, 0)
                    if (sampleSize < 0) {
                        codec.queueInputBuffer(
                            inputIndex,
                            0,
                            0,
                            0,
                            MediaCodec.BUFFER_FLAG_END_OF_STREAM,
                        )
                        sawInputEnd = true
                    } else {
                        codec.queueInputBuffer(inputIndex, 0, sampleSize, extractor.sampleTime, 0)
                        extractor.advance()
                    }
                }
            }

            val outputIndex = codec.dequeueOutputBuffer(bufferInfo, TIMEOUT_US)
            when {
                outputIndex >= 0 -> {
                    if (bufferInfo.size > 0) {
                        val outputBuffer = codec.getOutputBuffer(outputIndex)
                            ?: throw AnalysisDecodeException.DecodeFailed("no output buffer")
                        val bytes = ByteArray(bufferInfo.size)
                        outputBuffer.position(bufferInfo.offset)
                        outputBuffer.limit(bufferInfo.offset + bufferInfo.size)
                        outputBuffer.get(bytes)
                        val accepted = sink.pushPcmI16(
                            bytes,
                            sampleRateHz.toUInt(),
                            channelCount.toUInt(),
                        )
                        codec.releaseOutputBuffer(outputIndex, false)
                        if (!accepted) return
                    } else {
                        codec.releaseOutputBuffer(outputIndex, false)
                    }
                    if (bufferInfo.flags and MediaCodec.BUFFER_FLAG_END_OF_STREAM != 0) {
                        sawOutputEnd = true
                    }
                }
                outputIndex == MediaCodec.INFO_OUTPUT_FORMAT_CHANGED -> {
                    sampleRateHz = codec.outputFormat.sampleRateOrDefault()
                    channelCount = codec.outputFormat.channelCountOrDefault()
                }
                else -> Unit
            }
        }
    }

    private fun MediaFormat.sampleRateOrDefault(): Int =
        if (containsKey(MediaFormat.KEY_SAMPLE_RATE)) getInteger(MediaFormat.KEY_SAMPLE_RATE) else 0

    private fun MediaFormat.channelCountOrDefault(): Int =
        if (containsKey(MediaFormat.KEY_CHANNEL_COUNT)) getInteger(MediaFormat.KEY_CHANNEL_COUNT) else 0

    private companion object {
        const val TIMEOUT_US = 10_000L
    }
}
