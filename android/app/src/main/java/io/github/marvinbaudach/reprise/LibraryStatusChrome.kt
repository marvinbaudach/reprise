package io.github.marvinbaudach.reprise

import androidx.compose.animation.core.MutableTransitionState
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.MutableState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

@Composable
internal fun LibraryArtworkSummaryActions(
    tab: BrowseTab,
    summary: () -> String,
    searching: Boolean,
    toggleSearch: () -> Unit,
    rescan: () -> Unit,
    openSettings: () -> Unit,
    surfaceState: MobileSurfaceViewModel,
) {
    LibrarySummaryActions(
        tab = tab,
        summary = {
            summary() + artistPhotoProgressSummarySuffix(surfaceState.visibleArtistPhotoProgress)
        },
        searching = searching,
        toggleSearch = toggleSearch,
        rescan = rescan,
        openSettings = openSettings,
        artworkProgress = surfaceState.visibleArtistPhotoProgress,
        stopArtworkDownload = surfaceState::cancelArtistPhotoBackfill,
    )
}

@Composable
internal fun BoxScope.LibraryStatusChrome(
    browseError: String?,
    browseErrorOrigin: BrowseErrorOrigin?,
    surface: BrowseSurfaceGuard,
    dismissBrowseError: () -> Unit,
    surfaceState: MobileSurfaceViewModel,
    playback: LibraryPlayback,
    nowPlayingSheetState: MutableTransitionState<Boolean>,
    statusTopInset: MutableState<Dp>,
    detailPageIsTarget: () -> Boolean,
) {
    ArtistPhotoEdgeProgress(
        progress = surfaceState.visibleArtistPhotoProgress,
        dismiss = surfaceState::dismissArtistPhotoProgress,
        modifier = Modifier.align(Alignment.TopCenter).fillMaxWidth(),
    )
    LibraryStatusSlot(
        browseError = browseError,
        browseErrorOrigin = browseErrorOrigin,
        surface = surface,
        dismissBrowseError = dismissBrowseError,
        surfaceState = surfaceState,
        playback = playback,
        nowPlayingSheetState = nowPlayingSheetState,
        modifier = Modifier.align(Alignment.TopCenter).padding(
            top = libraryStatusTopPadding(statusTopInset.value, detailPageIsTarget()),
        ),
    )
}

internal fun libraryStatusTopPadding(inset: Dp, detailPageIsTarget: Boolean): Dp =
    8.dp + inset.takeIf { detailPageIsTarget }.orZero()

internal fun libraryStatusDetailInsetApplies(
    targetPage: BrowseTab,
    detailIsOpen: Boolean,
): Boolean = targetPage == BrowseTab.ARTISTS && detailIsOpen

private fun Dp?.orZero(): Dp = this ?: 0.dp
