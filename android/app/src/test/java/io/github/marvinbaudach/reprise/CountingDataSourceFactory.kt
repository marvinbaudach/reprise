package io.github.marvinbaudach.reprise

import android.net.Uri
import androidx.annotation.OptIn
import androidx.media3.common.util.UnstableApi
import androidx.media3.datasource.DataSource
import androidx.media3.datasource.DataSpec
import androidx.media3.datasource.TransferListener
import java.util.concurrent.atomic.AtomicLong

internal data class DataSourceCounts(
    val opens: Long,
    val bytesRead: Long,
)

@OptIn(UnstableApi::class)
internal class CountingDataSourceFactory(
    private val delegate: DataSource.Factory,
) : DataSource.Factory {
    private val opens = AtomicLong()
    private val bytesRead = AtomicLong()

    override fun createDataSource(): DataSource = CountingDataSource(delegate.createDataSource())

    fun counts(): DataSourceCounts = DataSourceCounts(
        opens = opens.get(),
        bytesRead = bytesRead.get(),
    )

    private inner class CountingDataSource(
        private val source: DataSource,
    ) : DataSource {
        override fun addTransferListener(transferListener: TransferListener) {
            source.addTransferListener(transferListener)
        }

        override fun open(dataSpec: DataSpec): Long {
            opens.incrementAndGet()
            return source.open(dataSpec)
        }

        override fun read(buffer: ByteArray, offset: Int, length: Int): Int =
            source.read(buffer, offset, length).also { read ->
                if (read > 0) bytesRead.addAndGet(read.toLong())
            }

        override fun getUri(): Uri? = source.uri

        override fun getResponseHeaders(): Map<String, List<String>> = source.responseHeaders

        override fun close() = source.close()
    }
}
