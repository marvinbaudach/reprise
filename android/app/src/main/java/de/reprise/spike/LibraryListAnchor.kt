package de.reprise.spike

import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.grid.LazyGridState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.snapshotFlow
import kotlinx.coroutines.flow.distinctUntilChanged

@Composable
internal fun ObserveLibraryListAnchor(
    key: LibraryListKey,
    state: LazyListState,
    surfaceState: MobileSurfaceViewModel,
) {
    LaunchedEffect(key, state, surfaceState) {
        snapshotFlow {
            LibraryScrollPosition(
                state.firstVisibleItemIndex,
                state.firstVisibleItemScrollOffset,
            )
        }.distinctUntilChanged().collect { position ->
            surfaceState.updateScroll(key, position)
        }
    }
    DisposableEffect(key, state, surfaceState) {
        onDispose {
            surfaceState.updateScroll(
                key,
                LibraryScrollPosition(
                    state.firstVisibleItemIndex,
                    state.firstVisibleItemScrollOffset,
                ),
            )
        }
    }
}

@Composable
internal fun ObserveLibraryGridAnchor(
    key: LibraryListKey,
    state: LazyGridState,
    surfaceState: MobileSurfaceViewModel,
) {
    LaunchedEffect(key, state, surfaceState) {
        snapshotFlow {
            LibraryScrollPosition(
                state.firstVisibleItemIndex,
                state.firstVisibleItemScrollOffset,
            )
        }.distinctUntilChanged().collect { position ->
            surfaceState.updateScroll(key, position)
        }
    }
    DisposableEffect(key, state, surfaceState) {
        onDispose {
            surfaceState.updateScroll(
                key,
                LibraryScrollPosition(
                    state.firstVisibleItemIndex,
                    state.firstVisibleItemScrollOffset,
                ),
            )
        }
    }
}
