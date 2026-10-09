package io.github.marvinbaudach.reprise.library

import android.net.Uri
import androidx.annotation.OptIn
import androidx.media3.common.C
import androidx.media3.common.util.UnstableApi
import androidx.media3.datasource.DataSource
import androidx.media3.extractor.Extractor
import androidx.media3.extractor.ExtractorInput
import androidx.media3.extractor.ExtractorOutput
import androidx.media3.extractor.ExtractorsFactory
import androidx.media3.extractor.PositionHolder
import androidx.media3.extractor.SeekMap
import androidx.media3.extractor.SniffFailure
import androidx.media3.extractor.TrackOutput

@OptIn(UnstableApi::class)
internal class UnknownDurationExtractorsFactory(
    private val delegate: ExtractorsFactory,
    private val reportUnknownDuration: Boolean = true,
    private val dataSourceFactory: DataSource.Factory? = null,
) : ExtractorsFactory {
    override fun createExtractors(): Array<Extractor> =
        delegate.createExtractors().wrapped(reportUnknownDuration, tailReader = null)

    override fun createExtractors(
        uri: Uri,
        responseHeaders: Map<String, List<String>>,
    ): Array<Extractor> = delegate.createExtractors(uri, responseHeaders).wrapped(
        reportUnknownDuration,
        tailReader = dataSourceFactory?.let { DataSourceFlacTailReader(it, uri) },
    )
}

@OptIn(UnstableApi::class)
private class UnknownDurationExtractor(
    private val delegate: Extractor,
    private val reportUnknownDuration: Boolean,
    private val tailReader: FlacTailReader?,
) : Extractor {
    private var maskingInput: FlacStreamInfoMaskingExtractorInput? = null

    override fun sniff(input: ExtractorInput): Boolean = delegate.sniff(input.maskedStreamInfo())

    override fun getSniffFailureDetails(): List<SniffFailure> = delegate.sniffFailureDetails

    override fun init(output: ExtractorOutput) {
        delegate.init(if (reportUnknownDuration) UnknownDurationExtractorOutput(output) else output)
    }

    override fun read(input: ExtractorInput, seekPosition: PositionHolder): Int =
        delegate.read(input.maskedStreamInfo(), seekPosition)

    override fun seek(position: Long, timeUs: Long) = delegate.seek(position, timeUs)

    override fun release() = delegate.release()

    override fun getUnderlyingImplementation(): Extractor = delegate.underlyingImplementation

    private fun ExtractorInput.maskedStreamInfo(): ExtractorInput {
        val current = maskingInput
        if (current != null && current.wraps(this)) return current
        return FlacStreamInfoMaskingExtractorInput(this, tailReader).also { maskingInput = it }
    }
}

@OptIn(UnstableApi::class)
private class UnknownDurationExtractorOutput(
    private val delegate: ExtractorOutput,
) : ExtractorOutput {
    override fun track(id: Int, type: Int): TrackOutput = delegate.track(id, type)

    override fun endTracks() = delegate.endTracks()

    override fun seekMap(seekMap: SeekMap) = delegate.seekMap(UnknownDurationSeekMap(seekMap))
}

@OptIn(UnstableApi::class)
private class UnknownDurationSeekMap(
    private val delegate: SeekMap,
) : SeekMap {
    override fun isSeekable(): Boolean = delegate.isSeekable

    override fun getDurationUs(): Long = C.TIME_UNSET

    override fun getSeekPoints(timeUs: Long): SeekMap.SeekPoints = delegate.getSeekPoints(timeUs)

    override fun isEstimated(): Boolean = delegate.isEstimated
}

@OptIn(UnstableApi::class)
private fun Array<Extractor>.wrapped(
    reportUnknownDuration: Boolean,
    tailReader: FlacTailReader?,
): Array<Extractor> =
    Array(size) { index -> UnknownDurationExtractor(this[index], reportUnknownDuration, tailReader) }
