package io.github.marvinbaudach.reprise

import android.util.Log
import java.util.Locale

internal const val VISUALIZER_LOG_TAG = "RepriseVisualizer"

/**
 * Permanent, edge-triggered diagnostics for the song-swipe bars (#1091).
 *
 * A device run reads these lines to name which mechanism emptied the bars around a swipe: a
 * `setPlaying(false)` blip through the item change, the old stream going stale, or an adoption
 * of an already decayed shape. Every function logs at an event, never per frame.
 */
internal object VisualizerEdgeLog {
    fun noteTrackChanged(trackId: Long) {
        Log.d(VISUALIZER_LOG_TAG, "noteTrackChanged track=$trackId")
    }

    /** [fromLastLive] is the engine's own word for the source, never inferred from equality. */
    fun adoptShape(adoptable: FloatArray, displayed: FloatArray, fromLastLive: Boolean) {
        Log.d(
            VISUALIZER_LOG_TAG,
            "adoptShape energy=${energy(adoptable)} displayedEnergy=${energy(displayed)} " +
                "fromLastLive=$fromLastLive",
        )
    }

    fun resetAudioStream() {
        Log.d(VISUALIZER_LOG_TAG, "resetAudioStream")
    }

    fun swipeCommit(decision: PlayGestureDecision, fromIndex: Int, toIndex: Int) {
        Log.d(VISUALIZER_LOG_TAG, "swipeCommit decision=$decision from=$fromIndex to=$toIndex")
    }

    private fun energy(bands: FloatArray): String = "%.2f".format(Locale.ROOT, bands.sum())
}

/** Logs a live panel's `setPlaying` value when it changes, with the snapshot state behind it. */
internal class VisualizerPlayingEdgeLog {
    private var last: Boolean? = null

    fun observe(playing: Boolean, playback: PlaybackUiState) {
        if (last == playing) return
        last = playing
        Log.d(VISUALIZER_LOG_TAG, "setPlaying($playing) snapshot=${playback.state}")
    }
}
