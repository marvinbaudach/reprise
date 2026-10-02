package io.github.marvinbaudach.reprise

import androidx.compose.animation.core.MutableTransitionState
import androidx.compose.runtime.Composable

/** Status rows that still reserve space above the library pager. */
@Composable
internal fun BrowseStatusLines(
    browseError: String?,
    browseErrorOrigin: BrowseErrorOrigin?,
    surface: BrowseSurfaceGuard,
    surfaceState: MobileSurfaceViewModel,
    playback: LibraryPlayback,
    nowPlayingSheetState: MutableTransitionState<Boolean>,
) {
    // Re-readable state, not timed acknowledgements; see TransientMessage.
    browseError
        ?.takeIf { browseErrorOrigin?.let(surface::errorOriginIsCurrent) != false }
        ?.let { BrowseErrorLine(it) }
    playback.error?.let { BrowseErrorLine(it) }
    if (
        !surfaceState.dockMode &&
        !nowPlayingSheetState.currentState &&
        !nowPlayingSheetState.targetState
    ) {
        playback.faultNotice?.let { BrowseErrorLine(it.text) }
    }
    ArtistPhotoProgressBar(
        progress = surfaceState.visibleArtistPhotoProgress,
        dismiss = surfaceState::dismissArtistPhotoProgress,
    )
}
