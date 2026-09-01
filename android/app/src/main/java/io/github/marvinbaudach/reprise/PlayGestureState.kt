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

/** Shared by the commit rule and the top-edge pre-indicator. */
internal const val TRACK_COMMIT_DISTANCE_FRACTION = 0.22f

/** The design's 0.55 physical px/ms, expressed in VelocityTracker's px/s unit. */
internal const val TRACK_FLING_PX_PER_SECOND = 550f

internal enum class NowPlayingPositionAction {
    SNAP,
    ANIMATE,
    REANCHOR,
    CONTINUE_SETTLE,
}

internal class NowPlayingPositionReconciler {
    private var trackId: Long? = null
    private var index: Int? = null

    fun update(
        trackId: Long,
        index: Int,
        dragging: Boolean,
        animationsEnabled: Boolean,
        settlingTargetIndex: Int? = null,
    ): NowPlayingPositionAction {
        val previousTrackId = this.trackId
        val previousIndex = this.index
        this.trackId = trackId
        this.index = index
        if (previousTrackId == null || !animationsEnabled) return NowPlayingPositionAction.SNAP
        if (dragging && previousIndex != index) return NowPlayingPositionAction.REANCHOR
        if (previousTrackId != trackId && settlingTargetIndex == index) {
            return NowPlayingPositionAction.CONTINUE_SETTLE
        }
        return if (previousTrackId != trackId) {
            NowPlayingPositionAction.ANIMATE
        } else {
            NowPlayingPositionAction.SNAP
        }
    }
}

internal class TrackChangeCueGate {
    private var trackId: Long? = null

    fun observe(trackId: Long, animationsEnabled: Boolean): Boolean {
        val previous = this.trackId
        this.trackId = trackId
        return animationsEnabled && previous != null && previous != trackId
    }
}

internal class ConfirmationCueTrigger {
    private var observedRevision = 0

    fun observe(cueRevision: Int, animationsEnabled: Boolean): Boolean {
        val changed = cueRevision > 0 && cueRevision != observedRevision
        observedRevision = cueRevision
        return changed && animationsEnabled
    }
}

internal typealias WaveformBuildTrigger = ConfirmationCueTrigger

internal class PlayGestureState(
    private val width: Float,
    private val height: Float,
    private val animationsEnabled: Boolean,
    currentIndex: Int,
    firstIndex: Int,
    lastIndex: Int,
) {
    private var horizontalAllowed = false
    private var verticalAllowed = false
    private var pendingX = 0f
    private var pendingY = 0f
    private var index = currentIndex
    private var firstIndex = firstIndex
    private var lastIndex = lastIndex
    private var dragDelta = 0f
    private var rawVerticalOffset = 0f

    var axis: PlayGestureAxis = PlayGestureAxis.NONE
        private set

    val positionPx: Float
        get() = if (animationsEnabled) dampedPosition(anchorPx + dragDelta) else anchorPx

    val deviationPx: Float
        get() = positionPx - anchorPx

    private val decisionDeviationPx: Float
        get() = dampedPosition(anchorPx + dragDelta) - anchorPx

    val verticalOffset: Float
        get() = if (animationsEnabled) rawVerticalOffset else 0f

    fun begin(horizontalAllowed: Boolean, verticalAllowed: Boolean) {
        this.horizontalAllowed = horizontalAllowed
        this.verticalAllowed = verticalAllowed
        pendingX = 0f
        pendingY = 0f
        dragDelta = 0f
        rawVerticalOffset = 0f
        axis = PlayGestureAxis.NONE
    }

    fun reanchor(
        currentIndex: Int,
        firstIndex: Int = this.firstIndex,
        lastIndex: Int = this.lastIndex,
    ) {
        index = currentIndex
        this.firstIndex = firstIndex
        this.lastIndex = lastIndex
        dragDelta = 0f
        pendingX = 0f
    }

    fun dragBy(deltaX: Float, deltaY: Float) {
        if (axis == PlayGestureAxis.NONE) {
            pendingX += deltaX
            pendingY += deltaY
            axis = chooseAxis()
            when (axis) {
                PlayGestureAxis.HORIZONTAL -> dragDelta = -pendingX
                PlayGestureAxis.VERTICAL -> rawVerticalOffset = pendingY.coerceAtLeast(0f)
                PlayGestureAxis.NONE -> return
            }
            return
        }

        when (axis) {
            PlayGestureAxis.HORIZONTAL -> dragDelta -= deltaX
            PlayGestureAxis.VERTICAL -> {
                rawVerticalOffset = (rawVerticalOffset + deltaY).coerceAtLeast(0f)
            }
            PlayGestureAxis.NONE -> Unit
        }
    }

    fun settle(velocityX: Float, velocityY: Float): PlayGestureDecision = when (axis) {
        PlayGestureAxis.HORIZONTAL -> when {
            canMoveNext && (
                decisionDeviationPx > width * TRACK_COMMIT_DISTANCE_FRACTION ||
                    velocityX < -TRACK_FLING_PX_PER_SECOND
                ) -> PlayGestureDecision.NEXT
            canMovePrevious && (
                decisionDeviationPx < -width * TRACK_COMMIT_DISTANCE_FRACTION ||
                    velocityX > TRACK_FLING_PX_PER_SECOND
                ) -> PlayGestureDecision.PREVIOUS
            else -> PlayGestureDecision.SPRING_BACK
        }
        PlayGestureAxis.VERTICAL -> when {
            rawVerticalOffset >= height * DISMISS_DISTANCE_FRACTION ||
                velocityY > TRACK_FLING_PX_PER_SECOND -> PlayGestureDecision.DISMISS
            else -> PlayGestureDecision.SPRING_BACK
        }
        PlayGestureAxis.NONE -> PlayGestureDecision.SPRING_BACK
    }

    private val anchorPx: Float
        get() = index * width

    private val canMovePrevious: Boolean
        get() = index > firstIndex

    private val canMoveNext: Boolean
        get() = index < lastIndex

    private fun dampedPosition(raw: Float): Float {
        val minimum = firstIndex * width
        val maximum = lastIndex * width
        return when {
            raw < minimum -> minimum + (raw - minimum) * END_RUBBER_BAND_FACTOR
            raw > maximum -> maximum + (raw - maximum) * END_RUBBER_BAND_FACTOR
            else -> raw
        }
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
        const val DISMISS_DISTANCE_FRACTION = 0.20f
        const val AXIS_LOCK_DISTANCE = 8f
        const val END_RUBBER_BAND_FACTOR = 0.3f
    }
}
