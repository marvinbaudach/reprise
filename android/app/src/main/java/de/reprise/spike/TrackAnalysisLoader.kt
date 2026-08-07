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
import uniffi.reprise_android_ffi.AndroidTrackRenderBar

private const val TAG = "RepriseAnalysis"
private const val SHUTDOWN_TIMEOUT_MS = 2_000L

/** One finished spectral cell. Rust owns its height and RGB channels. */
internal data class SpectralBar(
    val silence: Boolean,
    val level: Float,
    val red: Double,
    val green: Double,
    val blue: Double,
)

internal fun AndroidTrackRenderBar.toSpectralBar() = SpectralBar(
    silence = silence,
    level = level,
    red = red,
    green = green,
    blue = blue,
)

/** The analysis edge used by the playing-track lifecycle and seek surface. */
internal interface TrackAnalysisPort {
    /** Changes on the main thread after a sidecar import attempt completes. */
    val revision: Long

    fun prepare(trackId: Long)

    fun loadBars(trackId: Long, count: Int, deliver: (List<SpectralBar>?) -> Unit)
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
    private val onMainThread: (() -> Unit) -> Unit,
    private val worker: ExecutorService = singleAnalysisThread(),
) : TrackAnalysisPort {
    override var revision by mutableLongStateOf(0L)
        private set

    override fun prepare(trackId: Long) {
        submit("import analysis for track $trackId") {
            runCatching { importAnalysis(trackId) }
                .onFailure { error -> Log.w(TAG, "Could not import analysis for track $trackId", error) }
            onMainThread { revision += 1L }
        }
    }

    override fun loadBars(trackId: Long, count: Int, deliver: (List<SpectralBar>?) -> Unit) {
        submit("load analysis for track $trackId") {
            val bars = runCatching { readBars(trackId, count) }
                .onFailure { error -> Log.w(TAG, "Could not load analysis for track $trackId", error) }
                .getOrNull()
            onMainThread { deliver(bars) }
        }
    }

    private fun submit(description: String, work: () -> Unit) {
        try {
            worker.execute(work)
        } catch (rejected: RejectedExecutionException) {
            Log.d(TAG, "Not attempting to $description: the library is closing", rejected)
        }
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
        return drained
    }

    internal fun shutdownForTest() {
        check(shutdown()) { "analysis worker did not drain" }
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
