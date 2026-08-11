package de.reprise.spike

import org.junit.Assert.assertEquals
import org.junit.Test

class PlayGestureStateTest {
    @Test
    fun horizontal_distance_and_fling_cross_the_track_thresholds() {
        val distance = state().apply {
            begin(horizontalAllowed = true, verticalAllowed = true)
            dragBy(-101f, 4f)
        }
        val fling = state().apply {
            begin(horizontalAllowed = true, verticalAllowed = true)
            dragBy(-20f, 1f)
        }

        assertEquals(PlayGestureDecision.NEXT, distance.settle(velocityX = 0f, velocityY = 0f))
        assertEquals(PlayGestureDecision.NEXT, fling.settle(velocityX = -801f, velocityY = 0f))
    }

    @Test
    fun previous_and_downward_dismiss_use_their_signed_thresholds() {
        val previous = state().apply {
            begin(horizontalAllowed = true, verticalAllowed = true)
            dragBy(101f, 2f)
        }
        val dismiss = state().apply {
            begin(horizontalAllowed = false, verticalAllowed = true)
            dragBy(2f, 161f)
        }

        assertEquals(PlayGestureDecision.PREVIOUS, previous.settle(0f, 0f))
        assertEquals(PlayGestureDecision.DISMISS, dismiss.settle(0f, 0f))
    }

    @Test
    fun drag_below_every_threshold_springs_back() {
        val state = state().apply {
            begin(horizontalAllowed = true, verticalAllowed = true)
            dragBy(-99f, 3f)
        }

        assertEquals(PlayGestureDecision.SPRING_BACK, state.settle(0f, 0f))
    }

    @Test
    fun the_first_established_axis_stays_locked() {
        val state = state().apply {
            begin(horizontalAllowed = true, verticalAllowed = true)
            dragBy(20f, 2f)
            dragBy(0f, 180f)
        }

        assertEquals(PlayGestureAxis.HORIZONTAL, state.axis)
        assertEquals(20f, state.horizontalOffset)
        assertEquals(0f, state.verticalOffset)
    }

    @Test
    fun animations_off_keeps_visual_offsets_still_without_disabling_actions() {
        val state = state(animationsEnabled = false).apply {
            begin(horizontalAllowed = true, verticalAllowed = true)
            dragBy(-120f, 0f)
        }

        assertEquals(0f, state.horizontalOffset)
        assertEquals(0f, state.verticalOffset)
        assertEquals(PlayGestureDecision.NEXT, state.settle(0f, 0f))
    }

    @Test
    fun unavailable_axes_never_claim_the_gesture() {
        val state = state().apply {
            begin(horizontalAllowed = false, verticalAllowed = false)
            dragBy(-180f, 180f)
        }

        assertEquals(PlayGestureAxis.NONE, state.axis)
        assertEquals(PlayGestureDecision.SPRING_BACK, state.settle(-2_000f, 2_000f))
    }

    private fun state(animationsEnabled: Boolean = true) = PlayGestureState(
        width = 400f,
        height = 800f,
        flingThreshold = 800f,
        animationsEnabled = animationsEnabled,
    )
}
