package de.reprise.spike

import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.grid.LazyGridState
import androidx.compose.foundation.lazy.grid.rememberLazyGridState
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.runtime.snapshotFlow
import kotlin.math.roundToInt
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.first

/**
 * Where a library list is anchored: which item the viewport starts at, and how
 * far into that item it starts.
 *
 * The offset is a **fraction of that item** rather than a pixel count, because
 * the item is not the same height in both arrangements — a title row is 72 dp
 * stacked and 64 dp wide-short. Carrying 40 px across that change would mean
 * "40 px into a shorter row", which is a different place on the screen;
 * carrying "half a row" means the same place in both.
 */
internal data class LibraryScrollPosition(
    val firstVisibleItemIndex: Int = 0,
    val itemOffsetFraction: Float = 0f,
)

/** The anchor a list reports, with its pixel offset read against its own row. */
internal fun libraryScrollPosition(
    firstVisibleItemIndex: Int,
    firstVisibleItemScrollOffsetPx: Int,
    itemHeightPx: Int,
): LibraryScrollPosition = LibraryScrollPosition(
    firstVisibleItemIndex = firstVisibleItemIndex,
    itemOffsetFraction = if (itemHeightPx > 0) {
        firstVisibleItemScrollOffsetPx.toFloat() / itemHeightPx
    } else {
        0f
    },
)

internal fun LibraryScrollPosition.offsetPxIn(itemHeightPx: Int): Int =
    (itemOffsetFraction * itemHeightPx).roundToInt()

/**
 * The anchor to open a list of [itemCount] items with — or the top, when the
 * remembered item is not among them.
 *
 * A lazy list asked to start beyond its last item quietly starts at that last
 * item instead. That looks exactly like a restored position while being an
 * arbitrary one: a listener who left row 211 and is put on row 200 has been
 * told the place was kept. "Back at the top" reads as a reset, which is what it
 * is.
 */
internal fun LibraryScrollPosition.within(itemCount: Int): LibraryScrollPosition =
    if (firstVisibleItemIndex in 0 until itemCount) this else LibraryScrollPosition()

/**
 * A list state opened at [anchor].
 *
 * The index is applied up front, where a lazy list wants it. The fraction can
 * only be applied once the list has measured a row, because a fraction of an
 * unknown height is not a pixel count yet — so it is applied on the first
 * layout, and only while nothing else has moved the list in the meantime.
 */
@Composable
internal fun rememberLibraryListState(anchor: LibraryScrollPosition): LazyListState {
    val opening = remember { anchor }
    val state = rememberLazyListState(opening.firstVisibleItemIndex)
    LaunchedEffect(state) {
        if (opening.itemOffsetFraction == 0f) return@LaunchedEffect
        val itemHeightPx = snapshotFlow {
            state.layoutInfo.visibleItemsInfo.firstOrNull()?.size ?: 0
        }.first { height -> height > 0 }
        if (state.untouchedAt(opening.firstVisibleItemIndex)) {
            state.scrollToItem(
                opening.firstVisibleItemIndex,
                opening.offsetPxIn(itemHeightPx),
            )
        }
    }
    return state
}

/** [rememberLibraryListState] for the wide-short arrangement's two columns. */
@Composable
internal fun rememberLibraryGridState(anchor: LibraryScrollPosition): LazyGridState {
    val opening = remember { anchor }
    val state = rememberLazyGridState(opening.firstVisibleItemIndex)
    LaunchedEffect(state) {
        if (opening.itemOffsetFraction == 0f) return@LaunchedEffect
        val itemHeightPx = snapshotFlow {
            state.layoutInfo.visibleItemsInfo.firstOrNull()?.size?.height ?: 0
        }.first { height -> height > 0 }
        if (state.untouchedAt(opening.firstVisibleItemIndex)) {
            state.scrollToItem(
                opening.firstVisibleItemIndex,
                opening.offsetPxIn(itemHeightPx),
            )
        }
    }
    return state
}

private fun LazyListState.untouchedAt(index: Int): Boolean =
    firstVisibleItemIndex == index && firstVisibleItemScrollOffset == 0

private fun LazyGridState.untouchedAt(index: Int): Boolean =
    firstVisibleItemIndex == index && firstVisibleItemScrollOffset == 0

@Composable
internal fun ObserveLibraryListAnchor(
    key: LibraryListKey,
    state: LazyListState,
    surfaceState: MobileSurfaceViewModel,
) {
    LaunchedEffect(key, state, surfaceState) {
        snapshotFlow { state.anchor() }
            .distinctUntilChanged()
            .collect { position -> surfaceState.updateScroll(key, position) }
    }
    DisposableEffect(key, state, surfaceState) {
        onDispose { surfaceState.updateScroll(key, state.anchor()) }
    }
}

@Composable
internal fun ObserveLibraryGridAnchor(
    key: LibraryListKey,
    state: LazyGridState,
    surfaceState: MobileSurfaceViewModel,
) {
    LaunchedEffect(key, state, surfaceState) {
        snapshotFlow { state.anchor() }
            .distinctUntilChanged()
            .collect { position -> surfaceState.updateScroll(key, position) }
    }
    DisposableEffect(key, state, surfaceState) {
        onDispose { surfaceState.updateScroll(key, state.anchor()) }
    }
}

private fun LazyListState.anchor(): LibraryScrollPosition = libraryScrollPosition(
    firstVisibleItemIndex,
    firstVisibleItemScrollOffset,
    layoutInfo.visibleItemsInfo.firstOrNull()?.size ?: 0,
)

private fun LazyGridState.anchor(): LibraryScrollPosition = libraryScrollPosition(
    firstVisibleItemIndex,
    firstVisibleItemScrollOffset,
    layoutInfo.visibleItemsInfo.firstOrNull()?.size?.height ?: 0,
)
