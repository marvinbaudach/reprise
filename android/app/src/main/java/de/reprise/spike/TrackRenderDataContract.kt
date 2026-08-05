package de.reprise.spike

import androidx.compose.runtime.staticCompositionLocalOf

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
