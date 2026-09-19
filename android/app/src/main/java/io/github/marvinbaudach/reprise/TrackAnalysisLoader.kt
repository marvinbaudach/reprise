package io.github.marvinbaudach.reprise

import android.util.Log
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import java.util.concurrent.RejectedExecutionException
import java.util.concurrent.atomic.AtomicBoolean
import io.github.marvinbaudach.reprise.scene.SpectrogramFrames
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineName
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeoutOrNull
import uniffi.reprise_android_ffi.AndroidAnalysisOutcome
import uniffi.reprise_android_ffi.AndroidTrackRenderBar
import uniffi.reprise_android_ffi.AndroidTrackSpectrogram

private const val TAG = "RepriseAnalysis"
private const val SHUTDOWN_TIMEOUT_MS = 2_000L

private data class BarCacheKey(val trackId: Long, val count: Int)

/** One finished spectral cell. Rust owns its height and RGB channels. */
internal data class SpectralBar(
    val silence: Boolean,
    val level: Float,
    val red: Double,
    val green: Double,
    val blue: Double,
)

/** Finished analysis already available without another FFI/database read. */
internal data class TrackAnalysisWarmth(
    val bars: Boolean = false,
    val spectrogram: Boolean = false,
)

internal fun AndroidTrackRenderBar.toSpectralBar() = SpectralBar(
    silence = silence,
    level = level,
    red = red,
    green = green,
    blue = blue,
)

internal fun AndroidTrackSpectrogram.toSpectrogramFrames() = SpectrogramFrames(
    bandCount = bandCount.toInt(),
    frameRateHz = frameRateHz.toInt(),
    cells = cells,
)

/** The analysis edge used by the playing-track lifecycle and seek surface. */
internal interface TrackAnalysisPort {
    /** Changes on the main thread after a sidecar import attempt completes. */
    val revision: Long

    fun prepare(trackId: Long)

    fun loadBars(trackId: Long, count: Int, deliver: (List<SpectralBar>?) -> Unit)

    fun loadSpectrogram(trackId: Long, deliver: (SpectrogramFrames?) -> Unit) = deliver(null)

    fun prefetch(trackIds: List<Long>) = Unit

    fun retain(trackIds: Set<Long>) = Unit

    fun warmth(trackId: Long) = TrackAnalysisWarmth()
}

/**
 * Two independent background lanes: one for the lazy sidecar import/compute,
 * one for the finished-bar and spectrogram read. A computed analysis can
 * take seconds (decision 2 of
 * `docs/plans/the-phone-analyses-its-own-music.md`); sharing one lane with
 * it would queue every bar read on the seek bar behind that whole decode.
 *
 * The two lanes never need to observe each other's writes directly: the read
 * lane caches a `null` answer the same as a real one, and [invalidate] (run
 * on the main thread, from the import lane's own completion) clears exactly
 * that null entry so the next read on the same track is a real miss rather
 * than a stale negative cache hit. The revision this bumps is what lets a
 * read that started before the import finished retry once, without either
 * lane ever blocking on the other.
 */
internal class TrackAnalysisLoader(
    private val importAnalysis: (Long) -> AndroidAnalysisOutcome,
    private val readBars: (Long, Int) -> List<SpectralBar>?,
    private val readSpectrogram: (Long) -> AndroidTrackSpectrogram? = { null },
    private val onMainThread: (() -> Unit) -> Unit,
    private val importDispatcher: CoroutineDispatcher = analysisImportLane(),
    private val readDispatcher: CoroutineDispatcher = analysisReadLane(),
) : TrackAnalysisPort {
    private val accepting = AtomicBoolean(true)
    private val job = SupervisorJob()
    private val importScope =
        CoroutineScope(job + importDispatcher + CoroutineName("reprise-analysis-import"))
    private val readScope =
        CoroutineScope(job + readDispatcher + CoroutineName("reprise-analysis-read"))
    private val cacheLock = Any()
    private val barCache = mutableMapOf<BarCacheKey, List<SpectralBar>?>()
    private val barWaiters = mutableMapOf<BarCacheKey, MutableList<(List<SpectralBar>?) -> Unit>>()
    private val spectrogramCache = mutableMapOf<Long, SpectrogramFrames?>()
    private val spectrogramWaiters = mutableMapOf<Long, MutableList<(SpectrogramFrames?) -> Unit>>()
    private var retainedTrackIds: Set<Long>? = null
    private var preferredBarCount: Int? = null

    init {
        registerActive(this)
    }

    override var revision by mutableLongStateOf(0L)
        private set

    override fun prepare(trackId: Long) {
        submitImport("import analysis for track $trackId") {
            // Deliberately not logged on `AndroidAnalysisOutcome.COMPUTED`,
            // unlike the mirrored check in
            // `ReprisePlaybackService.trackAnalysisRequest`: this lane is
            // exercised by `TrackAnalysisLoaderTest`, a plain JUnit test with
            // no Robolectric runner, where an unshadowed `android.util.Log`
            // call throws — a real, reproduced failure
            // (`aComputedAnalysisRefreshesTheBars`), not a hypothetical one.
            // `ReprisePlaybackService` already logs the same message for
            // every real, foreground-triggered compute, so nothing is lost
            // on the one path that matters. The `Log.w` below is pre-existing
            // (unchanged from before this file's two-lane split) and stays
            // unexercised by these tests, since `importAnalysis` here never
            // throws — it is not the same kind of risk.
            try {
                importAnalysis(trackId)
            } catch (cancelled: CancellationException) {
                throw cancelled
            } catch (error: Throwable) {
                Log.w(TAG, "Could not import analysis for track $trackId", error)
            }
            onMainThread {
                invalidate(trackId)
                revision += 1L
            }
        }
    }

    override fun loadBars(trackId: Long, count: Int, deliver: (List<SpectralBar>?) -> Unit) {
        val key = BarCacheKey(trackId, count)
        val cached = synchronized(cacheLock) {
            val firstKnownBarCount = preferredBarCount == null
            preferredBarCount = count
            if (barCache.containsKey(key)) {
                Triple(true, barCache[key], firstKnownBarCount)
            } else {
                val waiters = barWaiters[key]
                if (waiters != null) {
                    waiters += deliver
                    return
                }
                barWaiters[key] = mutableListOf(deliver)
                Triple(false, null, firstKnownBarCount)
            }
        }
        if (cached.first) {
            // Cache hits deliver on the caller's thread. The only background caller is
            // prefetch(), which supplies an empty callback. Accepted misses hop below;
            // shutdown rejection completes inline with null.
            deliver(cached.second)
            if (cached.third) warmRetainedBars(count)
            return
        }
        val submittedRevision = revision
        val submitted = submitRead("load analysis for track $trackId") {
            val bars = try {
                readBars(trackId, count)
            } catch (cancelled: CancellationException) {
                throw cancelled
            } catch (error: Throwable) {
                Log.w(TAG, "Could not load analysis for track $trackId", error)
                null
            }
            onMainThread { finishBarLoad(key, bars, cache = true, submittedRevision) }
        }
        if (!submitted) {
            finishBarLoad(key, bars = null, cache = false, submittedRevision)
        }
        if (cached.third) warmRetainedBars(count)
    }

    override fun loadSpectrogram(trackId: Long, deliver: (SpectrogramFrames?) -> Unit) {
        val cached = synchronized(cacheLock) {
            if (spectrogramCache.containsKey(trackId)) {
                true to spectrogramCache[trackId]
            } else {
                val waiters = spectrogramWaiters[trackId]
                if (waiters != null) {
                    waiters += deliver
                    return
                }
                spectrogramWaiters[trackId] = mutableListOf(deliver)
                false to null
            }
        }
        if (cached.first) {
            // Cache hits deliver on the caller's thread. The only background caller is
            // prefetch(), which supplies an empty callback. Accepted misses hop below;
            // shutdown rejection completes inline with null.
            deliver(cached.second)
            return
        }
        val submittedRevision = revision
        val submitted = submitRead("load spectrogram for track $trackId") {
            val frames = try {
                readSpectrogram(trackId)?.toSpectrogramFrames()
            } catch (cancelled: CancellationException) {
                throw cancelled
            } catch (error: Throwable) {
                Log.w(TAG, "Could not load spectrogram for track $trackId", error)
                null
            }
            onMainThread { finishSpectrogramLoad(trackId, frames, cache = true, submittedRevision) }
        }
        if (!submitted) {
            finishSpectrogramLoad(trackId, frames = null, cache = false, submittedRevision)
        }
    }

    override fun prefetch(trackIds: List<Long>) {
        val barCount = synchronized(cacheLock) { preferredBarCount }
        trackIds.forEach { trackId ->
            if (barCount != null) loadBars(trackId, barCount) {}
            loadSpectrogram(trackId) {}
        }
    }

    override fun retain(trackIds: Set<Long>) {
        synchronized(cacheLock) {
            retainedTrackIds = trackIds.toSet()
            barCache.keys.removeAll { key -> key.trackId !in trackIds }
            spectrogramCache.keys.retainAll(trackIds)
        }
    }

    override fun warmth(trackId: Long) = synchronized(cacheLock) {
        // There is one bar-count consumer today, so warmth follows its latest requested count.
        // A second count-owning consumer needs a count-aware warmth contract instead.
        TrackAnalysisWarmth(
            bars = preferredBarCount?.let { count ->
                barCache[BarCacheKey(trackId, count)] != null
            } == true,
            spectrogram = spectrogramCache[trackId] != null,
        )
    }

    /**
     * `submittedRevision` is [revision] as it stood when this read was
     * submitted. A `null` result is only cached if `revision` has not moved
     * since then: import and read run on independent lanes with no relative
     * ordering guarantee, so a read that observes the database before a
     * concurrent import's write can finish, and post here, *after* that
     * import already ran [invalidate] — which found nothing to clear because
     * the cache entry did not exist yet. Caching the null unconditionally at
     * that point would pin a stale negative that no further invalidate is
     * scheduled to clear. A non-null result is never stale in that sense and
     * is always cached.
     */
    private fun finishBarLoad(
        key: BarCacheKey,
        bars: List<SpectralBar>?,
        cache: Boolean,
        submittedRevision: Long,
    ) {
        val waiters = synchronized(cacheLock) {
            val supersededNegative = bars == null && submittedRevision != revision
            if (cache && !supersededNegative && retainedTrackIds?.contains(key.trackId) != false) {
                barCache[key] = bars
            }
            barWaiters.remove(key).orEmpty()
        }
        waiters.forEach { deliver -> deliver(bars) }
    }

    /** See [finishBarLoad]: the same race applies to the spectrogram cache. */
    private fun finishSpectrogramLoad(
        trackId: Long,
        frames: SpectrogramFrames?,
        cache: Boolean,
        submittedRevision: Long,
    ) {
        val waiters = synchronized(cacheLock) {
            val supersededNegative = frames == null && submittedRevision != revision
            if (cache && !supersededNegative && retainedTrackIds?.contains(trackId) != false) {
                spectrogramCache[trackId] = frames
            }
            spectrogramWaiters.remove(trackId).orEmpty()
        }
        waiters.forEach { deliver -> deliver(frames) }
    }

    private fun invalidate(trackId: Long) {
        synchronized(cacheLock) {
            barCache.entries.removeAll { (key, bars) -> key.trackId == trackId && bars == null }
            if (spectrogramCache[trackId] == null) spectrogramCache.remove(trackId)
        }
    }

    private fun warmRetainedBars(count: Int) {
        val trackIds = synchronized(cacheLock) { retainedTrackIds.orEmpty() }
        trackIds.forEach { trackId -> loadBars(trackId, count) {} }
    }

    private fun submitImport(description: String, work: suspend () -> Unit): Boolean =
        submitTo(importScope, description, work)

    private fun submitRead(description: String, work: suspend () -> Unit): Boolean =
        submitTo(readScope, description, work)

    private fun submitTo(
        scope: CoroutineScope,
        description: String,
        work: suspend () -> Unit,
    ): Boolean {
        if (!accepting.get() || !scope.isActive) {
            val rejected = RejectedExecutionException("analysis loader is shut down")
            Log.d(TAG, "Not attempting to $description: the library is closing", rejected)
            return false
        }
        scope.launch { work() }
        return true
    }

    /** Stops accepting work and lets both lanes' already started work finish. */
    fun shutdown(): Boolean {
        accepting.set(false)
        job.complete()
        val drained = try {
            runBlocking {
                withTimeoutOrNull(SHUTDOWN_TIMEOUT_MS) {
                    job.join()
                    true
                } != null
            }
        } catch (interrupted: InterruptedException) {
            Thread.currentThread().interrupt()
            false
        }
        if (!drained) {
            importScope.cancel()
            readScope.cancel()
        }
        unregisterActive(this)
        return drained
    }

    internal fun shutdownForTest() {
        check(shutdown()) { "analysis worker did not drain" }
    }

    companion object {
        private val activeLock = Any()
        private var active: TrackAnalysisLoader? = null

        internal fun activePort(): TrackAnalysisPort? = synchronized(activeLock) { active }

        private fun registerActive(loader: TrackAnalysisLoader) {
            synchronized(activeLock) { active = loader }
        }

        private fun unregisterActive(loader: TrackAnalysisLoader) {
            synchronized(activeLock) {
                if (active === loader) active = null
            }
        }
    }
}

@OptIn(ExperimentalCoroutinesApi::class)
private fun analysisImportLane(): CoroutineDispatcher = Dispatchers.IO.limitedParallelism(1)

@OptIn(ExperimentalCoroutinesApi::class)
private fun analysisReadLane(): CoroutineDispatcher = Dispatchers.IO.limitedParallelism(1)

internal val LocalTrackAnalysis = staticCompositionLocalOf<TrackAnalysisPort> {
    object : TrackAnalysisPort {
        override val revision = 0L
        override fun prepare(trackId: Long) = Unit
        override fun loadBars(
            trackId: Long,
            count: Int,
            deliver: (List<SpectralBar>?) -> Unit,
        ) = deliver(null)
    }
}
