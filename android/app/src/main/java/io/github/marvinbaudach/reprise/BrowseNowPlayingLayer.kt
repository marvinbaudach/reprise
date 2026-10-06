package io.github.marvinbaudach.reprise

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.MutableTransitionState
import androidx.compose.animation.expandVertically
import androidx.compose.animation.shrinkVertically
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.asPaddingValues
import androidx.compose.foundation.layout.calculateStartPadding
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.NavigationRailDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp

/**
 * What sits over the library: the Now Playing sheet, or the dock surface that
 * replaces the whole screen, and the offer to enter dock mode.
 */
@Composable
internal fun BoxScope.BrowseNowPlayingLayer(
    surfaceState: MobileSurfaceViewModel,
    surfaceLayout: SurfaceLayout,
    playback: LibraryPlayback,
    nowPlayingPlayback: () -> PlaybackUiState,
    shownTrack: LibraryTrack?,
    shownTrackIsStale: Boolean,
    nowPlayingSheetState: MutableTransitionState<Boolean>,
    settingsVisible: Boolean,
) {
    val frameMetrics = libraryFrameMetrics(surfaceLayout)
    val nowPlayingFrameModifier = when (surfaceLayout) {
        SurfaceLayout.STACKED -> Modifier
            .fillMaxSize()
        SurfaceLayout.WIDE_SHORT -> Modifier
            .fillMaxSize()
            .padding(
                start = frameMetrics.navigationRailWidthDp.dp +
                    NavigationRailDefaults.windowInsets
                        .asPaddingValues()
                        .calculateStartPadding(LocalLayoutDirection.current),
            )
    }
    if (surfaceState.dockMode) {
        shownTrack?.let { track ->
            CompositionLocalProvider(
                LocalNowPlayingActionsEnabled provides !shownTrackIsStale,
            ) {
                DockModeSurface(track, playback, surfaceState)
            }
        } ?: DockModeWaitingSurface()
    } else {
        AnimatedVisibility(
            // The row is part of the condition, not just of the content: a
            // sheet that slides up around nothing — which is what a stop
            // followed straight away by a new track used to do, the answer
            // for the new row still being read — pops its content in
            // afterwards, with no animation of its own.
            visibleState = nowPlayingSheetState,
            modifier = nowPlayingFrameModifier.testTag("now-playing-frame"),
            enter = slideInVertically(initialOffsetY = { height -> height }) + expandVertically(
                expandFrom = Alignment.Bottom,
            ),
            exit = slideOutVertically(targetOffsetY = { height -> height }) + shrinkVertically(
                shrinkTowards = Alignment.Bottom,
            ),
        ) {
            shownTrack?.let { track ->
                CompositionLocalProvider(
                    LocalNowPlayingActionsEnabled provides !shownTrackIsStale,
                ) {
                    NowPlayingSheet(
                        track = track,
                        playback = nowPlayingPlayback(),
                        surfaceLayout = surfaceLayout,
                        surfaceState = surfaceState,
                        close = { surfaceState.showNowPlaying(false) },
                    )
                }
            }
        }
    }
    if (
        surfaceState.dockOfferVisible &&
        surfaceLayout == SurfaceLayout.WIDE_SHORT &&
        shownTrack != null &&
        !surfaceState.dockMode &&
        !settingsVisible
    ) {
        Button(
            onClick = surfaceState::enterDockMode,
            modifier = Modifier
                .align(Alignment.TopEnd)
                .padding(12.dp),
        ) {
            Text("Dock mode")
        }
    }
}
