package io.github.marvinbaudach.reprise

import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.composed
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.positionChange
import androidx.compose.ui.unit.dp
import kotlin.math.abs
import kotlin.math.max
import kotlinx.coroutines.Job
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

internal data class PlayPanel(
    val index: Int,
    val track: LibraryTrack,
)

internal data class PlayPanelWindow(
    val panels: List<PlayPanel>,
    val firstIndex: Int,
    val lastIndex: Int,
)

internal fun placeholderPlayPanelWindow(
    track: LibraryTrack,
    currentIndex: Int,
): PlayPanelWindow = PlayPanelWindow(
    panels = listOf(PlayPanel(currentIndex, track)),
    firstIndex = currentIndex,
    lastIndex = currentIndex,
)

internal fun PlayPanelWindow.withCurrentPanel(
    track: LibraryTrack,
    currentIndex: Int,
): PlayPanelWindow {
    val indexIsKnown = currentIndex in firstIndex..lastIndex
    return PlayPanelWindow(
        panels = if (indexIsKnown) {
            (panels.filter { panel ->
                panel.index != currentIndex && abs(panel.index - currentIndex) <= 1
            } + PlayPanel(currentIndex, track)).sortedBy(PlayPanel::index)
        } else {
            listOf(PlayPanel(currentIndex, track))
        },
        firstIndex = if (indexIsKnown) firstIndex else currentIndex,
        lastIndex = if (indexIsKnown) lastIndex else currentIndex,
    )
}

internal fun playPanelWindow(
    currentIndex: Int,
    currentTrackId: Long,
    rows: List<LibraryTrack>,
): PlayPanelWindow {
    val currentRow = rows.indexOfFirst { row -> row.id == currentTrackId }
    if (currentRow < 0) return PlayPanelWindow(emptyList(), currentIndex, currentIndex)
    val indexedRows = rows.mapIndexed { rowIndex, row ->
        PlayPanel(index = currentIndex + rowIndex - currentRow, track = row)
    }
    return PlayPanelWindow(
        panels = indexedRows.filter { panel -> abs(panel.index - currentIndex) <= 1 },
        firstIndex = indexedRows.first().index,
        lastIndex = indexedRows.last().index,
    )
}

@Composable
internal fun rememberPlayPanelWindow(
    track: LibraryTrack,
    currentIndex: Int,
    controls: PlaybackControls,
): PlayPanelWindow {
    var generation by remember { mutableStateOf(0L) }
    var window by remember {
        mutableStateOf(placeholderPlayPanelWindow(track, currentIndex))
    }
    LaunchedEffect(track.id, currentIndex, controls) {
        val requestGeneration = ++generation
        window = window.withCurrentPanel(track, currentIndex)
        controls.loadUpcomingTracks(LibraryWindowRange(-2, 3)) { outcome ->
            if (generation != requestGeneration) return@loadUpcomingTracks
            outcome.getOrNull()?.rows?.let { rows ->
                window = playPanelWindow(currentIndex, track.id, rows).takeIf {
                    it.panels.isNotEmpty()
                } ?: window
            }
        }
    }
    return window
}

internal fun Modifier.nowPlayingGestures(
    animationsEnabled: Boolean,
    currentIndex: Int,
    firstIndex: Int,
    lastIndex: Int,
    positionPx: Float = Float.NaN,
    onHorizontalPosition: (Float) -> Unit,
    onVerticalOffset: (Float) -> Unit,
    onDragStateChanged: (Boolean) -> Unit,
    onSettle: (PlayGestureDecision) -> Unit,
    onDoubleTap: (leftHalf: Boolean) -> Unit,
    onTap: (Offset) -> Unit,
): Modifier = composed {
    val latestCurrentIndex by rememberUpdatedState(currentIndex)
    val latestFirstIndex by rememberUpdatedState(firstIndex)
    val latestLastIndex by rememberUpdatedState(lastIndex)
    val latestPositionPx by rememberUpdatedState(positionPx)
    pointerInput(animationsEnabled) {
    val transportHeight = TRANSPORT_EXCLUSION_DP.dp.toPx()
    val doubleTapDistance = DOUBLE_TAP_DISTANCE_DP.dp.toPx()
    coroutineScope {
        var lastTapTime = Long.MIN_VALUE
        var lastTapPosition = Offset.Unspecified
        var pendingTap: Job? = null

        awaitEachGesture {
            val down = awaitFirstDown(requireUnconsumed = false, pass = PointerEventPass.Main)
            val startY = down.position.y
            val horizontalAllowed = startY <= size.height * COVER_GESTURE_FRACTION
            val verticalAllowed = startY !in (size.height * SEEK_EXCLUSION_START)..
                (size.height * SEEK_EXCLUSION_END) && startY < size.height - transportHeight
            val state = PlayGestureState(
                width = size.width.toFloat(),
                height = size.height.toFloat(),
                animationsEnabled = animationsEnabled,
                currentIndex = latestCurrentIndex,
                firstIndex = latestFirstIndex,
                lastIndex = latestLastIndex,
            ).apply { begin(horizontalAllowed, verticalAllowed) }
            var stateIndex = latestCurrentIndex
            var velocityOriginTime = down.uptimeMillis
            var velocityOriginPosition = down.position
            var dragged = false
            var childConsumed = down.isConsumed
            var upTime = down.uptimeMillis
            var upPosition = down.position
            onDragStateChanged(true)
            // A new pointer stream owns the one position immediately. This also
            // seats a no-op transport commit if it interrupted the prior settle.
            if (latestPositionPx.isFinite() && latestPositionPx != state.positionPx) {
                onHorizontalPosition(state.positionPx)
            }

            try {
                while (true) {
                    val event = awaitPointerEvent(PointerEventPass.Main)
                    val change = event.changes.firstOrNull { it.id == down.id } ?: break
                    val delta = change.positionChange()
                    if (stateIndex != latestCurrentIndex) {
                        stateIndex = latestCurrentIndex
                        state.reanchor(stateIndex, latestFirstIndex, latestLastIndex)
                        onHorizontalPosition(state.positionPx)
                        velocityOriginTime = change.uptimeMillis
                        velocityOriginPosition = change.position
                    }
                    childConsumed = childConsumed || change.isConsumed
                    if (!change.isConsumed && change.pressed && delta != Offset.Zero) {
                        state.dragBy(delta.x, delta.y)
                        if (state.axis != PlayGestureAxis.NONE) {
                            dragged = true
                            onHorizontalPosition(state.positionPx)
                            onVerticalOffset(state.verticalOffset)
                        }
                    }
                    upTime = change.uptimeMillis
                    upPosition = change.position
                    // This parent observes children first, then consumes the remainder so
                    // the library pager behind the sheet never receives the same stream.
                    change.consume()
                    if (!change.pressed) break
                }

                if (dragged) {
                    pendingTap?.cancel()
                    pendingTap = null
                    val measured = gestureVelocityPxPerSecond(
                        displacement = upPosition - velocityOriginPosition,
                        elapsedMs = upTime - velocityOriginTime,
                    )
                    onSettle(state.settle(measured.x, measured.y))
                    lastTapTime = Long.MIN_VALUE
                    lastTapPosition = Offset.Unspecified
                } else {
                    val isDoubleTap = horizontalAllowed && !childConsumed &&
                        lastTapTime != Long.MIN_VALUE &&
                        upTime - lastTapTime <= DOUBLE_TAP_TIMEOUT_MS &&
                        abs(upPosition.x - lastTapPosition.x) <= doubleTapDistance &&
                        abs(upPosition.y - lastTapPosition.y) <= doubleTapDistance
                    if (isDoubleTap) {
                        pendingTap?.cancel()
                        pendingTap = null
                        onDoubleTap(upPosition.x < size.width / 2f)
                        lastTapTime = Long.MIN_VALUE
                        lastTapPosition = Offset.Unspecified
                    } else if (horizontalAllowed && !childConsumed) {
                        lastTapTime = upTime
                        lastTapPosition = upPosition
                        val tapTime = upTime
                        val tapPosition = upPosition
                        pendingTap = launch {
                            delay(DOUBLE_TAP_TIMEOUT_MS)
                            onTap(tapPosition)
                            if (lastTapTime == tapTime) {
                                lastTapTime = Long.MIN_VALUE
                                lastTapPosition = Offset.Unspecified
                                pendingTap = null
                            }
                        }
                    }
                }
            } finally {
                onDragStateChanged(false)
            }
        }
    }
}
}

internal fun gestureVelocityPxPerSecond(displacement: Offset, elapsedMs: Long): Offset {
    val seconds = max(MINIMUM_FLING_SAMPLE_MS, elapsedMs).toFloat() / 1_000f
    return displacement / seconds
}

private const val COVER_GESTURE_FRACTION = 0.62f
private const val SEEK_EXCLUSION_START = 0.64f
private const val SEEK_EXCLUSION_END = 0.76f
private const val TRANSPORT_EXCLUSION_DP = 132
private const val DOUBLE_TAP_TIMEOUT_MS = 300L
private const val DOUBLE_TAP_DISTANCE_DP = 48
private const val MINIMUM_FLING_SAMPLE_MS = 60L
