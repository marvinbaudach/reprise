package de.reprise.spike

import androidx.compose.runtime.staticCompositionLocalOf

internal interface LibraryPerformanceObserver {
    fun trackRowComposed(trackId: Long, presentation: TrackPlaybackPresentation) = Unit

    fun miniPlayerProgressDrawn(progress: Float) = Unit
}

internal object NoOpLibraryPerformanceObserver : LibraryPerformanceObserver

internal val LocalLibraryPerformanceObserver = staticCompositionLocalOf<LibraryPerformanceObserver> {
    NoOpLibraryPerformanceObserver
}
