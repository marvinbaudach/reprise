package io.github.marvinbaudach.reprise.library

import android.content.Context
import androidx.annotation.OptIn
import androidx.media3.common.MediaItem
import androidx.media3.common.util.UnstableApi
import androidx.media3.datasource.DataSource
import androidx.media3.datasource.DefaultDataSource
import androidx.media3.exoplayer.drm.DrmSessionManagerProvider
import androidx.media3.exoplayer.source.DefaultMediaSourceFactory
import androidx.media3.exoplayer.source.MediaSource
import androidx.media3.exoplayer.upstream.LoadErrorHandlingPolicy
import androidx.media3.extractor.DefaultExtractorsFactory

/**
 * Builds ordinary Media3 sources, but lets Media3 learn the decoded duration
 * of a last CUE track instead of trusting the container header.
 */
@OptIn(UnstableApi::class)
internal class OpenEndedMediaSourceFactory(
    context: Context,
    dataSourceFactory: DataSource.Factory = DefaultDataSource.Factory(context),
    reportUnknownDuration: Boolean = true,
) : MediaSource.Factory {
    private val normalDelegate = DefaultMediaSourceFactory(dataSourceFactory)
    private val lenientDelegate = DefaultMediaSourceFactory(
        dataSourceFactory,
        UnknownDurationExtractorsFactory(DefaultExtractorsFactory(), reportUnknownDuration, dataSourceFactory),
    )

    override fun setDrmSessionManagerProvider(
        drmSessionManagerProvider: DrmSessionManagerProvider,
    ): MediaSource.Factory {
        normalDelegate.setDrmSessionManagerProvider(drmSessionManagerProvider)
        lenientDelegate.setDrmSessionManagerProvider(drmSessionManagerProvider)
        return this
    }

    override fun setLoadErrorHandlingPolicy(
        loadErrorHandlingPolicy: LoadErrorHandlingPolicy,
    ): MediaSource.Factory {
        normalDelegate.setLoadErrorHandlingPolicy(loadErrorHandlingPolicy)
        lenientDelegate.setLoadErrorHandlingPolicy(loadErrorHandlingPolicy)
        return this
    }

    override fun getSupportedTypes(): IntArray = normalDelegate.supportedTypes

    override fun createMediaSource(mediaItem: MediaItem): MediaSource {
        val request = mediaItem.localConfiguration?.tag as? PlaybackRequest
        val segment = request?.segment
        val delegate = if (segment != null && segment.endMs == null) {
            lenientDelegate
        } else {
            normalDelegate
        }
        return delegate.createMediaSource(mediaItem)
    }
}
