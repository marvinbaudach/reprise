package io.github.marvinbaudach.reprise

import androidx.compose.ui.geometry.Offset
import org.junit.Assert.assertEquals
import org.junit.Test

class PlayGestureStateTest {
    @Test
    fun horizontal_distance_and_fling_cross_the_track_thresholds() {
        val distance = state().apply {
            begin(horizontalAllowed = true, verticalAllowed = true)
            dragBy(-89f, 4f)
        }
        val fling = state().apply {
            begin(horizontalAllowed = true, verticalAllowed = true)
            dragBy(-20f, 1f)
        }

        assertEquals(PlayGestureDecision.NEXT, distance.settle(velocityX = 0f, velocityY = 0f))
        assertEquals(PlayGestureDecision.NEXT, fling.settle(velocityX = -551f, velocityY = 0f))
    }

    @Test
    fun very_short_flings_use_the_sixty_millisecond_velocity_floor() {
        val velocity = gestureVelocityPxPerSecond(Offset(-33f, 12f), elapsedMs = 10)

        assertEquals(-550f, velocity.x)
        assertEquals(200f, velocity.y)
    }

    @Test
    fun the_exact_commit_boundary_does_not_overpromise() {
        val state = state().apply {
            begin(horizontalAllowed = true, verticalAllowed = true)
            dragBy(-88f, 0f)
        }

        assertEquals(PlayGestureDecision.SPRING_BACK, state.settle(0f, 0f))
        assertEquals(88f, state.deviationPx)
    }

    @Test
    fun position_is_absolute_and_end_overscroll_is_damped() {
        val middle = state(currentIndex = 2, firstIndex = 0, lastIndex = 4).apply {
            begin(horizontalAllowed = true, verticalAllowed = true)
            dragBy(-40f, 0f)
        }
        val beforeFirst = state(currentIndex = 0, firstIndex = 0, lastIndex = 4).apply {
            begin(horizontalAllowed = true, verticalAllowed = true)
            dragBy(100f, 0f)
        }
        val afterLast = state(currentIndex = 4, firstIndex = 0, lastIndex = 4).apply {
            begin(horizontalAllowed = true, verticalAllowed = true)
            dragBy(-100f, 0f)
        }

        assertEquals(840f, middle.positionPx)
        assertEquals(-30f, beforeFirst.positionPx, 0.000_01f)
        assertEquals(1_630f, afterLast.positionPx, 0.000_01f)
    }

    @Test
    fun previous_and_downward_dismiss_use_their_signed_thresholds() {
        val previous = state().apply {
            begin(horizontalAllowed = true, verticalAllowed = true)
            dragBy(89f, 2f)
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
            dragBy(-87f, 3f)
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
        assertEquals(380f, state.positionPx)
        assertEquals(0f, state.verticalOffset)
    }

    @Test
    fun animations_off_keeps_visual_offsets_still_without_disabling_actions() {
        val state = state(animationsEnabled = false).apply {
            begin(horizontalAllowed = true, verticalAllowed = true)
            dragBy(-120f, 0f)
        }

        assertEquals(400f, state.positionPx)
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

    private fun state(
        animationsEnabled: Boolean = true,
        currentIndex: Int = 1,
        firstIndex: Int = 0,
        lastIndex: Int = 2,
    ) = PlayGestureState(
        width = 400f,
        height = 800f,
        animationsEnabled = animationsEnabled,
        currentIndex = currentIndex,
        firstIndex = firstIndex,
        lastIndex = lastIndex,
    )
}
