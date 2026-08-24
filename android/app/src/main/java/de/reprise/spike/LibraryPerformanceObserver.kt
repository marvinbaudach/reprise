package de.reprise.spike

import androidx.compose.runtime.staticCompositionLocalOf

/**
 * Test seam for guarding library recomposition and mini-player draw work.
 * Production uses the NoOp implementation because only regression tests need
 * these measurements.
 */
internal interface LibraryPerformanceObserver {
    fun trackRowComposed(trackId: Long, presentation: TrackPlaybackPresentation) = Unit

    fun miniPlayerProgressDrawn(progress: Float) = Unit
}

internal object NoOpLibraryPerformanceObserver : LibraryPerformanceObserver

internal val LocalLibraryPerformanceObserver = staticCompositionLocalOf<LibraryPerformanceObserver> {
    NoOpLibraryPerformanceObserver
}
