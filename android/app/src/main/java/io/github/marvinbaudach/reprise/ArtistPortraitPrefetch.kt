package io.github.marvinbaudach.reprise

import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors
import java.util.concurrent.RejectedExecutionException
import uniffi.reprise_android_ffi.AndroidArtworkSize

private const val PORTRAIT_PREFETCH_BATCH_SIZE = 32u

internal class ArtistPortraitPrefetch(
    private val port: LibrarySessionPort,
    private val worker: ExecutorService = singlePortraitPrefetchThread(),
) {
    @Volatile
    private var stopped = false

    fun start() {
        if (stopped) return
        try {
            worker.execute(::fetchMissingPortraits)
        } catch (_: RejectedExecutionException) {
            // Shutdown won the race with this start request.
        }
    }

    fun shutdown() {
        stopped = true
        worker.shutdownNow()
    }

    private fun fetchMissingPortraits() {
        val attempted = mutableSetOf<String>()
        var requestLimit = PORTRAIT_PREFETCH_BATCH_SIZE
        while (!stopped) {
            val names = runCatching {
                port.artistsMissingPortraits(requestLimit)
            }.getOrElse { return }
            if (names.isEmpty()) return

            var foundNewName = false
            for (name in names) {
                if (!attempted.add(name)) continue
                foundNewName = true
                if (stopped) return
                runCatching {
                    port.artistPortraitFetched(name, AndroidArtworkSize.LIST)
                }
            }
            if (!foundNewName) {
                if (names.size < requestLimit.toInt()) return
                val expandedLimit = minOf(
                    Int.MAX_VALUE.toUInt(),
                    requestLimit + PORTRAIT_PREFETCH_BATCH_SIZE,
                )
                if (expandedLimit == requestLimit) return
                requestLimit = expandedLimit
            }
        }
    }
}

private fun singlePortraitPrefetchThread(): ExecutorService =
    Executors.newSingleThreadExecutor { runnable ->
        Thread(runnable, "reprise-artist-portraits")
    }
