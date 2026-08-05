package de.reprise.spike

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.os.PowerManager
import androidx.activity.ComponentActivity
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.core.content.ContextCompat
import uniffi.reprise_android_ffi.AndroidTrackAnalysisOutcome
import uniffi.reprise_android_ffi.MusicLibrary
import uniffi.reprise_android_ffi.TrackAnalysisSession

internal interface MainActivityAnalysisProvider {
    fun mainActivityAnalysis(): MainActivityAnalysisDependencies
}

internal data class MainActivityAnalysisDependencies(
    val renderData: TrackRenderDataPort,
    val coordinator: TrackAnalysisCoordinator,
    val release: () -> Unit = {},
)

internal fun activityTrackAnalysis(
    activity: ComponentActivity,
    surfaceProvided: Boolean,
    library: () -> MusicLibrary,
): MainActivityAnalysisDependencies =
    (activity.application as? MainActivityAnalysisProvider)?.mainActivityAnalysis()
        ?: if (surfaceProvided) noTrackAnalysis() else productionTrackAnalysis(activity, library)

internal fun productionTrackAnalysis(
    activity: ComponentActivity,
    library: () -> MusicLibrary,
): MainActivityAnalysisDependencies {
    val onMainThread: (() -> Unit) -> Unit = { work -> activity.runOnUiThread(work) }
    val renderData = LibraryTrackRenderData(library, onMainThread)
    val backend = StreamingTrackAnalysisBackend(
        beginSession = { trackId, _ ->
            FfiTrackAnalysisSession(TrackAnalysisSession.begin(library(), trackId))
        },
        decoder = AndroidMediaCodecTrackDecoder(activity),
        onMainThread = onMainThread,
    )
    return MainActivityAnalysisDependencies(
        renderData = renderData,
        coordinator = TrackAnalysisController(backend, renderData),
        release = renderData::shutdown,
    )
}

internal fun noTrackAnalysis(): MainActivityAnalysisDependencies =
    MainActivityAnalysisDependencies(NoTrackRenderData, NoTrackAnalysisCoordinator)

@Composable
internal fun BindTrackAnalysis(
    coordinator: TrackAnalysisCoordinator,
    sheetOpen: Boolean,
    playback: PlaybackUiState,
) {
    LaunchedEffect(sheetOpen, playback.currentTrackId, playback.currentTrackUri) {
        coordinator.observe(sheetOpen, playback.currentTrackId, playback.currentTrackUri)
    }
}

private object NoTrackAnalysisCoordinator : TrackAnalysisCoordinator {
    override fun observe(sheetOpen: Boolean, trackId: Long?, contentUri: String?) = Unit
    override fun surfaceActive(active: Boolean) = Unit
    override fun shutdown() = Unit
}

private class FfiTrackAnalysisSession(
    private val session: TrackAnalysisSession,
) : TrackAnalysisSessionPort {
    override fun push(interleaved: List<Float>, sampleRateHz: UInt, channelCount: UInt) =
        session.push(interleaved, sampleRateHz, channelCount)

    override fun finish(): AndroidTrackAnalysisOutcome = session.finish()
    override fun cancel() = session.cancel()
    override fun close() = session.close()
}

/** Delivers the activity and physical-screen visibility boundary to analysis. */
internal class TrackAnalysisVisibility(
    private val activity: ComponentActivity,
    private val coordinator: TrackAnalysisCoordinator,
) {
    private val power = activity.getSystemService(Context.POWER_SERVICE) as PowerManager
    private var resumed = false
    private var registered = false
    private val screenReceiver = object : BroadcastReceiver() {
        override fun onReceive(context: Context, intent: Intent) {
            when (intent.action) {
                Intent.ACTION_SCREEN_OFF -> coordinator.surfaceActive(false)
                Intent.ACTION_SCREEN_ON -> coordinator.surfaceActive(resumed && power.isInteractive)
            }
        }
    }

    fun start() {
        if (registered) return
        ContextCompat.registerReceiver(
            activity,
            screenReceiver,
            IntentFilter().apply {
                addAction(Intent.ACTION_SCREEN_OFF)
                addAction(Intent.ACTION_SCREEN_ON)
            },
            ContextCompat.RECEIVER_NOT_EXPORTED,
        )
        registered = true
    }

    fun resume() {
        resumed = true
        coordinator.surfaceActive(power.isInteractive)
    }

    fun pause() {
        resumed = false
        coordinator.surfaceActive(false)
    }

    fun shutdown() {
        coordinator.shutdown()
        if (registered) {
            activity.unregisterReceiver(screenReceiver)
            registered = false
        }
    }
}
