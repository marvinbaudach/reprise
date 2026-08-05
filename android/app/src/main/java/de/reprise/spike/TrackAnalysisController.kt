package de.reprise.spike

import android.util.Log
import uniffi.reprise_android_ffi.AndroidTrackAnalysisOutcome

private const val TAG = "Reprise"

internal typealias TrackAnalysisResult = Result<AndroidTrackAnalysisOutcome>

internal fun interface TrackAnalysisWork {
    fun cancel()
}

internal interface TrackAnalysisBackend {
    /** Starts one complete decode pass and answers on the main thread. */
    fun start(
        trackId: Long,
        contentUri: String,
        deliver: (TrackAnalysisResult) -> Unit,
    ): TrackAnalysisWork
}

internal interface TrackAnalysisRenderDataStore {
    /** Checks availability off the main thread and answers back on it. */
    fun hasData(trackId: Long, deliver: (Result<Boolean>) -> Unit)

    /** Publishes that a complete analysis has atomically landed. */
    fun analysisStored(trackId: Long)
}

internal interface TrackAnalysisCoordinator {
    fun observe(sheetOpen: Boolean, trackId: Long?, contentUri: String?)
    fun surfaceActive(active: Boolean)
    fun shutdown()
}

/** Keeps one decode pass scoped to the one visible Now Playing track. */
internal class TrackAnalysisController(
    private val backend: TrackAnalysisBackend,
    private val renderData: TrackAnalysisRenderDataStore,
    /**
     * Where a pass that produced nothing says so. It is a seam rather than a
     * direct `Log.w` because the reporting is the behaviour under test: a
     * silent failure is what let a phone draw a flat bar forever with nothing
     * in the log to explain it.
     */
    private val report: (String) -> Unit = { message -> Log.w(TAG, message) },
) : TrackAnalysisCoordinator {
    private data class Target(val trackId: Long, val contentUri: String)

    private var sheetOpen = false
    private var surfaceActive = false
    private var trackId: Long? = null
    private var contentUri: String? = null
    private var currentTarget: Target? = null
    private var currentWork: TrackAnalysisWork? = null
    private var generation = 0L
    private var shutDown = false

    override fun observe(sheetOpen: Boolean, trackId: Long?, contentUri: String?) {
        this.sheetOpen = sheetOpen
        this.trackId = trackId
        this.contentUri = contentUri
        reconcile()
    }

    override fun surfaceActive(active: Boolean) {
        surfaceActive = active
        reconcile()
    }

    override fun shutdown() {
        shutDown = true
        reconcile()
    }

    private fun reconcile() {
        val desired = if (!shutDown && sheetOpen && surfaceActive) {
            val id = trackId
            val uri = contentUri
            if (id != null && uri != null) Target(id, uri) else null
        } else {
            null
        }
        if (desired == currentTarget) return

        generation += 1L
        currentWork?.cancel()
        currentWork = null
        currentTarget = desired
        val target = desired ?: return
        val requestGeneration = generation
        renderData.hasData(target.trackId) { answer ->
            if (requestGeneration != generation || currentTarget != target) return@hasData
            if (answer.getOrElse { true }) return@hasData
            currentWork = backend.start(target.trackId, target.contentUri) { outcome ->
                if (requestGeneration != generation || currentTarget != target) return@start
                currentWork = null
                if (outcome.getOrNull() == AndroidTrackAnalysisOutcome.STORED) {
                    renderData.analysisStored(target.trackId)
                } else {
                    // An analysis that produces nothing leaves a flat bar, which
                    // is indistinguishable from one that has not started. On a
                    // phone whose library was MP3 and Opus that state lasted
                    // forever and said nothing — the log is where the next
                    // person finds out why.
                    report(
                        "no rendering data for track ${target.trackId}: " +
                            (
                                outcome.exceptionOrNull()?.toString()
                                    ?: outcome.getOrNull()?.toString()
                                    ?: "the pass ended without an answer"
                                ),
                    )
                }
            }
        }
    }
}
