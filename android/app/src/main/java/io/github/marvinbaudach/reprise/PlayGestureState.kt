package io.github.marvinbaudach.reprise

import kotlin.math.abs

internal enum class PlayGestureAxis {
    NONE,
    HORIZONTAL,
    VERTICAL,
}

internal enum class PlayGestureDecision {
    NEXT,
    PREVIOUS,
    DISMISS,
    SPRING_BACK,
}

internal class PlayGestureState(
    private val width: Float,
    private val height: Float,
    private val flingThreshold: Float,
    private val animationsEnabled: Boolean,
) {
    private var horizontalAllowed = false
    private var verticalAllowed = false
    private var pendingX = 0f
    private var pendingY = 0f
    private var rawHorizontalOffset = 0f
    private var rawVerticalOffset = 0f

    var axis: PlayGestureAxis = PlayGestureAxis.NONE
        private set

    val horizontalOffset: Float
        get() = if (animationsEnabled) rawHorizontalOffset else 0f

    val verticalOffset: Float
        get() = if (animationsEnabled) rawVerticalOffset else 0f

    fun begin(horizontalAllowed: Boolean, verticalAllowed: Boolean) {
        this.horizontalAllowed = horizontalAllowed
        this.verticalAllowed = verticalAllowed
        pendingX = 0f
        pendingY = 0f
        rawHorizontalOffset = 0f
        rawVerticalOffset = 0f
        axis = PlayGestureAxis.NONE
    }

    fun dragBy(deltaX: Float, deltaY: Float) {
        if (axis == PlayGestureAxis.NONE) {
            pendingX += deltaX
            pendingY += deltaY
            axis = chooseAxis()
            when (axis) {
                PlayGestureAxis.HORIZONTAL -> rawHorizontalOffset = pendingX
                PlayGestureAxis.VERTICAL -> rawVerticalOffset = pendingY.coerceAtLeast(0f)
                PlayGestureAxis.NONE -> return
            }
            return
        }

        when (axis) {
            PlayGestureAxis.HORIZONTAL -> rawHorizontalOffset += deltaX
            PlayGestureAxis.VERTICAL -> {
                rawVerticalOffset = (rawVerticalOffset + deltaY).coerceAtLeast(0f)
            }
            PlayGestureAxis.NONE -> Unit
        }
    }

    fun settle(velocityX: Float, velocityY: Float): PlayGestureDecision = when (axis) {
        PlayGestureAxis.HORIZONTAL -> when {
            rawHorizontalOffset <= -width * TRACK_DISTANCE_FRACTION ||
                velocityX <= -flingThreshold -> PlayGestureDecision.NEXT
            rawHorizontalOffset >= width * TRACK_DISTANCE_FRACTION ||
                velocityX >= flingThreshold -> PlayGestureDecision.PREVIOUS
            else -> PlayGestureDecision.SPRING_BACK
        }
        PlayGestureAxis.VERTICAL -> when {
            rawVerticalOffset >= height * DISMISS_DISTANCE_FRACTION ||
                velocityY >= flingThreshold -> PlayGestureDecision.DISMISS
            else -> PlayGestureDecision.SPRING_BACK
        }
        PlayGestureAxis.NONE -> PlayGestureDecision.SPRING_BACK
    }

    private fun chooseAxis(): PlayGestureAxis {
        val horizontalDistance = abs(pendingX)
        val verticalDistance = abs(pendingY)
        if (horizontalDistance < AXIS_LOCK_DISTANCE && verticalDistance < AXIS_LOCK_DISTANCE) {
            return PlayGestureAxis.NONE
        }
        if (horizontalAllowed && horizontalDistance >= verticalDistance) {
            return PlayGestureAxis.HORIZONTAL
        }
        if (verticalAllowed && pendingY > 0f && verticalDistance > horizontalDistance) {
            return PlayGestureAxis.VERTICAL
        }
        if (horizontalAllowed && !verticalAllowed) {
            return PlayGestureAxis.HORIZONTAL
        }
        return PlayGestureAxis.NONE
    }

    private companion object {
        const val TRACK_DISTANCE_FRACTION = 0.25f
        const val DISMISS_DISTANCE_FRACTION = 0.20f
        const val AXIS_LOCK_DISTANCE = 8f
    }
}
