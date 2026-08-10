package de.reprise.spike

import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.positionChange
import androidx.compose.ui.input.pointer.util.VelocityTracker
import androidx.compose.ui.unit.dp
import kotlin.math.abs

internal data class PlayGestureNeighbours(
    val previous: LibraryTrack?,
    val next: LibraryTrack?,
)

@Composable
internal fun rememberPlayGestureNeighbours(
    track: LibraryTrack,
    controls: PlaybackControls,
): PlayGestureNeighbours {
    var rememberedTrack by remember { mutableStateOf(track) }
    var previous by remember { mutableStateOf<LibraryTrack?>(null) }
    var next by remember { mutableStateOf<LibraryTrack?>(null) }
    LaunchedEffect(track.id, controls) {
        if (rememberedTrack.id != track.id) {
            previous = rememberedTrack
            rememberedTrack = track
        }
        next = null
        controls.loadUpcomingTracks(LibraryWindowRange(0, 2)) { outcome ->
            next = outcome.getOrNull()?.rows?.firstOrNull()
        }
    }
    return PlayGestureNeighbours(previous = previous, next = next)
}

internal fun Modifier.nowPlayingGestures(
    animationsEnabled: Boolean,
    onHorizontalOffset: (Float) -> Unit,
    onVerticalOffset: (Float) -> Unit,
    onSettle: (PlayGestureDecision) -> Unit,
    onDoubleTap: (leftHalf: Boolean) -> Unit,
): Modifier = pointerInput(animationsEnabled) {
    val flingThreshold = TRACK_FLING_DP_PER_SECOND.dp.toPx()
    val transportHeight = TRANSPORT_EXCLUSION_DP.dp.toPx()
    val doubleTapDistance = DOUBLE_TAP_DISTANCE_DP.dp.toPx()
    var lastTapTime = Long.MIN_VALUE
    var lastTapPosition = Offset.Unspecified

    awaitEachGesture {
        val down = awaitFirstDown(requireUnconsumed = false, pass = PointerEventPass.Main)
        val startY = down.position.y
        val horizontalAllowed = startY <= size.height * COVER_GESTURE_FRACTION
        val verticalAllowed = startY !in (size.height * SEEK_EXCLUSION_START)..
            (size.height * SEEK_EXCLUSION_END) && startY < size.height - transportHeight
        val state = PlayGestureState(
            width = size.width.toFloat(),
            height = size.height.toFloat(),
            flingThreshold = flingThreshold,
            animationsEnabled = animationsEnabled,
        ).apply { begin(horizontalAllowed, verticalAllowed) }
        val velocity = VelocityTracker().apply { addPosition(down.uptimeMillis, down.position) }
        var dragged = false
        var childConsumed = down.isConsumed
        var upTime = down.uptimeMillis
        var upPosition = down.position

        while (true) {
            val event = awaitPointerEvent(PointerEventPass.Main)
            val change = event.changes.firstOrNull { it.id == down.id } ?: break
            val delta = change.positionChange()
            childConsumed = childConsumed || change.isConsumed
            if (!change.isConsumed && change.pressed && delta != Offset.Zero) {
                state.dragBy(delta.x, delta.y)
                if (state.axis != PlayGestureAxis.NONE) {
                    dragged = true
                    onHorizontalOffset(state.horizontalOffset)
                    onVerticalOffset(state.verticalOffset)
                }
            }
            velocity.addPosition(change.uptimeMillis, change.position)
            upTime = change.uptimeMillis
            upPosition = change.position
            // This parent observes children first, then consumes the remainder so
            // the library pager behind the sheet never receives the same stream.
            change.consume()
            if (!change.pressed) break
        }

        if (dragged) {
            val measured = velocity.calculateVelocity()
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
                onDoubleTap(upPosition.x < size.width / 2f)
                lastTapTime = Long.MIN_VALUE
                lastTapPosition = Offset.Unspecified
            } else {
                lastTapTime = upTime
                lastTapPosition = upPosition
            }
        }
    }
}

private const val COVER_GESTURE_FRACTION = 0.62f
private const val SEEK_EXCLUSION_START = 0.64f
private const val SEEK_EXCLUSION_END = 0.76f
private const val TRACK_FLING_DP_PER_SECOND = 800
private const val TRANSPORT_EXCLUSION_DP = 132
private const val DOUBLE_TAP_TIMEOUT_MS = 300L
private const val DOUBLE_TAP_DISTANCE_DP = 48
