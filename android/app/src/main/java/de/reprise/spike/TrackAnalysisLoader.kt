package de.reprise.spike

import android.util.Log
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors
import java.util.concurrent.RejectedExecutionException
import java.util.concurrent.TimeUnit
import de.reprise.spike.scene.SpectrogramFrames
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
 * One ordered background lane for the lazy sidecar import and finished-bar read.
 *
 * Import and read share a worker because the read must observe the database
 * write that precedes it. The revision makes an early no-data read retry after
 * the import completes without ever blocking Compose or duplicating Rust's
 * shaping and colour work here.
 */
internal class TrackAnalysisLoader(
    private val importAnalysis: (Long) -> Unit,
    private val readBars: (Long, Int) -> List<SpectralBar>?,
    private val readSpectrogram: (Long) -> AndroidTrackSpectrogram? = { null },
    private val onMainThread: (() -> Unit) -> Unit,
    private val worker: ExecutorService = singleAnalysisThread(),
) : TrackAnalysisPort {
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
        submit("import analysis for track $trackId") {
            runCatching { importAnalysis(trackId) }
                .onFailure { error -> Log.w(TAG, "Could not import analysis for track $trackId", error) }
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
        val submitted = submit("load analysis for track $trackId") {
            val bars = runCatching { readBars(trackId, count) }
                .onFailure { error -> Log.w(TAG, "Could not load analysis for track $trackId", error) }
                .getOrNull()
            onMainThread { finishBarLoad(key, bars, cache = true) }
        }
        if (!submitted) {
            finishBarLoad(key, bars = null, cache = false)
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
        val submitted = submit("load spectrogram for track $trackId") {
            val frames = runCatching { readSpectrogram(trackId)?.toSpectrogramFrames() }
                .onFailure { error ->
                    Log.w(TAG, "Could not load spectrogram for track $trackId", error)
                }
                .getOrNull()
            onMainThread { finishSpectrogramLoad(trackId, frames, cache = true) }
        }
        if (!submitted) {
            finishSpectrogramLoad(trackId, frames = null, cache = false)
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

    private fun finishBarLoad(
        key: BarCacheKey,
        bars: List<SpectralBar>?,
        cache: Boolean,
    ) {
        val waiters = synchronized(cacheLock) {
            if (cache && retainedTrackIds?.contains(key.trackId) != false) {
                barCache[key] = bars
            }
            barWaiters.remove(key).orEmpty()
        }
        waiters.forEach { deliver -> deliver(bars) }
    }

    private fun finishSpectrogramLoad(
        trackId: Long,
        frames: SpectrogramFrames?,
        cache: Boolean,
    ) {
        val waiters = synchronized(cacheLock) {
            if (cache && retainedTrackIds?.contains(trackId) != false) {
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

    private fun submit(description: String, work: () -> Unit): Boolean = try {
        worker.execute(work)
        true
    } catch (rejected: RejectedExecutionException) {
        Log.d(TAG, "Not attempting to $description: the library is closing", rejected)
        false
    }

    /** Stops accepting work and lets the already ordered import/read pair finish. */
    fun shutdown(): Boolean {
        worker.shutdown()
        val drained = try {
            worker.awaitTermination(SHUTDOWN_TIMEOUT_MS, TimeUnit.MILLISECONDS)
        } catch (interrupted: InterruptedException) {
            Thread.currentThread().interrupt()
            false
        }
        if (!drained) worker.shutdownNow()
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

private fun singleAnalysisThread(): ExecutorService =
    Executors.newSingleThreadExecutor { runnable -> Thread(runnable, "reprise-analysis") }

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
