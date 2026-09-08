package io.github.marvinbaudach.reprise

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineName
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import uniffi.reprise_android_ffi.AndroidArtworkSize

private const val PORTRAIT_PREFETCH_BATCH_SIZE = 32u

internal class ArtistPortraitPrefetch(
    private val port: LibrarySessionPort,
    private val dispatcher: CoroutineDispatcher = portraitPrefetchLane(),
) {
    private val job = SupervisorJob()
    private val scope = CoroutineScope(job + dispatcher + CoroutineName("reprise-artist-portraits"))

    fun start() {
        if (!scope.isActive) return
        scope.launch { fetchMissingPortraits() }
    }

    fun shutdown() {
        scope.cancel()
    }

    private suspend fun fetchMissingPortraits() {
        val attempted = mutableSetOf<String>()
        var requestLimit = PORTRAIT_PREFETCH_BATCH_SIZE
        while (true) {
            currentCoroutineContext().ensureActive()
            val names = try {
                port.artistsMissingPortraits(requestLimit)
            } catch (cancelled: CancellationException) {
                throw cancelled
            } catch (_: Throwable) {
                return
            }
            currentCoroutineContext().ensureActive()
            if (names.isEmpty()) return

            var foundNewName = false
            for (name in names) {
                if (!attempted.add(name)) continue
                foundNewName = true
                currentCoroutineContext().ensureActive()
                try {
                    port.artistPortraitFetched(name, AndroidArtworkSize.LIST)
                } catch (cancelled: CancellationException) {
                    throw cancelled
                } catch (_: Throwable) {
                    // A failed portrait does not stop the rest of the batch.
                }
                currentCoroutineContext().ensureActive()
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

@OptIn(ExperimentalCoroutinesApi::class)
private fun portraitPrefetchLane(): CoroutineDispatcher = Dispatchers.IO.limitedParallelism(1)
