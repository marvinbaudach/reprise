package de.reprise.spike

import android.util.Log
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.staticCompositionLocalOf
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean
import uniffi.reprise_android_ffi.MusicLibrary

/** One shaped seek-bar cell, coloured in Rust so both frontends share one axis. */
internal data class TrackRenderBar(
    val silence: Boolean,
    val level: Float,
    val red: Float,
    val green: Float,
    val blue: Float,
)

internal interface TrackRenderDataPort {
    /**
     * Bumped whenever rendering data has newly landed for any track. Read it
     * before asking for bars so a surface recomposes when an analysis finishes.
     */
    val revision: Int

    /** `null` means "not analysed" — the flat bar. An empty list means analysed and empty. */
    fun bars(trackId: Long, barCount: Int): List<TrackRenderBar>?

    /** The 24 stored band bytes (0..255) nearest `position` in 0..1, or `null` when unanalysed. */
    fun spectrumColumn(trackId: Long, position: Float): List<Int>?
}

/** Previews and any surface without an activity draw the flat bar. */
internal object NoTrackRenderData : TrackRenderDataPort {
    override val revision = 0
    override fun bars(trackId: Long, barCount: Int): List<TrackRenderBar>? = null
    override fun spectrumColumn(trackId: Long, position: Float): List<Int>? = null
}

internal val LocalTrackRenderData =
    staticCompositionLocalOf<TrackRenderDataPort> { NoTrackRenderData }

private const val RENDER_DATA_TAG = "RepriseRenderData"
private const val MAX_SPECTRUM_CACHE_ENTRIES = 64

private data class BarsKey(val trackId: Long, val barCount: Int, val generation: Int)
private data class SpectrumKey(val trackId: Long, val positionBits: Int, val generation: Int)

/**
 * Activity-owned, asynchronous cache in front of the library's UniFFI reads.
 *
 * [bars] and [spectrumColumn] never cross UniFFI on the calling thread. A cache
 * miss returns the honest flat/absent state, schedules one read, and publishes
 * its answer by changing [revision] on the main thread.
 */
internal class LibraryTrackRenderData(
    private val readBars: (Long, Int) -> List<TrackRenderBar>?,
    private val readSpectrum: (Long, Float) -> ByteArray?,
    private val onMainThread: (() -> Unit) -> Unit,
    private val executor: ExecutorService = renderDataExecutor(),
) : TrackRenderDataPort, TrackAnalysisRenderDataStore {
    private val revisionState = mutableIntStateOf(0)
    private val closed = AtomicBoolean(false)
    private val trackGenerations = mutableMapOf<Long, Int>()
    private val barsCache = mutableMapOf<BarsKey, List<TrackRenderBar>?>()
    private val pendingBars = mutableSetOf<BarsKey>()
    private val spectrumCache = LinkedHashMap<SpectrumKey, List<Int>?>()
    private val pendingSpectrum = mutableSetOf<SpectrumKey>()

    constructor(
        library: () -> MusicLibrary,
        onMainThread: (() -> Unit) -> Unit,
    ) : this(
        readBars = { trackId, barCount ->
            library().trackRenderBars(trackId, barCount.toUInt())?.map { bar ->
                TrackRenderBar(
                    silence = bar.silence,
                    level = bar.level,
                    red = bar.red.toFloat(),
                    green = bar.green.toFloat(),
                    blue = bar.blue.toFloat(),
                )
            }
        },
        readSpectrum = { trackId, position ->
            library().trackSpectrumColumn(trackId, position.toDouble())
        },
        onMainThread = onMainThread,
    )

    override val revision: Int
        get() = revisionState.intValue

    override fun bars(trackId: Long, barCount: Int): List<TrackRenderBar>? {
        val key = BarsKey(trackId, barCount.coerceAtLeast(0), generation(trackId))
        synchronized(barsCache) {
            if (barsCache.containsKey(key)) return barsCache[key]
            if (!pendingBars.add(key) || closed.get()) return null
        }
        executor.execute {
            val answer = runCatching { readBars(key.trackId, key.barCount) }
            publishBars(key, answer)
        }
        return null
    }

    override fun spectrumColumn(trackId: Long, position: Float): List<Int>? {
        val bounded = position.coerceIn(0f, 1f)
        val key = SpectrumKey(trackId, bounded.toBits(), generation(trackId))
        synchronized(spectrumCache) {
            if (spectrumCache.containsKey(key)) return spectrumCache[key]
            if (!pendingSpectrum.add(key) || closed.get()) return null
        }
        executor.execute {
            val answer = runCatching {
                readSpectrum(trackId, bounded)?.map { value -> value.toUByte().toInt() }
            }
            publishSpectrum(key, answer)
        }
        return null
    }

    override fun hasData(trackId: Long, deliver: (Result<Boolean>) -> Unit) {
        if (closed.get()) {
            onMainThread { deliver(Result.failure(IllegalStateException("render data is closed"))) }
            return
        }
        executor.execute {
            val answer = runCatching { readBars(trackId, 1) != null }
            if (!closed.get()) onMainThread { deliver(answer) }
        }
    }

    override fun analysisStored(trackId: Long) {
        synchronized(trackGenerations) {
            trackGenerations[trackId] = generation(trackId) + 1
        }
        synchronized(barsCache) {
            barsCache.keys.removeAll { key -> key.trackId == trackId }
        }
        synchronized(spectrumCache) {
            spectrumCache.keys.removeAll { key -> key.trackId == trackId }
        }
        revisionState.intValue += 1
    }

    fun shutdown() {
        if (closed.compareAndSet(false, true)) executor.shutdownNow()
    }

    private fun publishBars(key: BarsKey, answer: Result<List<TrackRenderBar>?>) {
        if (closed.get()) return
        onMainThread {
            synchronized(barsCache) {
                pendingBars.remove(key)
                answer.onSuccess { barsCache[key] = it }
            }
            answer.onSuccess { bars -> if (bars != null) revisionState.intValue += 1 }
                .onFailure { error -> Log.w(RENDER_DATA_TAG, "Could not read track bars", error) }
        }
    }

    private fun generation(trackId: Long): Int =
        synchronized(trackGenerations) { trackGenerations[trackId] ?: 0 }

    private fun publishSpectrum(key: SpectrumKey, answer: Result<List<Int>?>) {
        if (closed.get()) return
        onMainThread {
            synchronized(spectrumCache) {
                pendingSpectrum.remove(key)
                answer.onSuccess { spectrumCache[key] = it }
                while (spectrumCache.size > MAX_SPECTRUM_CACHE_ENTRIES) {
                    spectrumCache.remove(spectrumCache.keys.first())
                }
            }
            answer.onSuccess { column -> if (column != null) revisionState.intValue += 1 }
                .onFailure { error -> Log.w(RENDER_DATA_TAG, "Could not read spectrum", error) }
        }
    }
}

private fun renderDataExecutor(): ExecutorService =
    Executors.newSingleThreadExecutor { work ->
        Thread(work, "reprise-render-data").apply { isDaemon = true }
    }
