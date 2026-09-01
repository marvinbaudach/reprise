package io.github.marvinbaudach.reprise

import org.junit.Assert.assertEquals
import org.junit.Test

class NowPlayingPositionStateTest {
    @Test
    fun external_advance_mid_drag_reanchors_before_the_finger_continues() {
        val state = PlayGestureState(
            width = 400f,
            height = 800f,
            animationsEnabled = true,
            currentIndex = 4,
            firstIndex = 0,
            lastIndex = 8,
        ).apply {
            begin(horizontalAllowed = true, verticalAllowed = true)
            dragBy(-70f, 0f)
        }

        state.reanchor(5)

        assertEquals(2_000f, state.positionPx, 0f)
        assertEquals(0f, state.deviationPx, 0f)
        state.dragBy(-20f, 0f)
        assertEquals(20f, state.deviationPx, 0f)
        assertEquals(PlayGestureDecision.SPRING_BACK, state.settle(0f, 0f))
    }

    @Test
    fun same_track_queue_edit_reseats_without_motion() {
        val reconciler = NowPlayingPositionReconciler()
        assertEquals(
            NowPlayingPositionAction.SNAP,
            reconciler.update(trackId = 40, index = 2, dragging = false, animationsEnabled = true),
        )

        assertEquals(
            NowPlayingPositionAction.SNAP,
            reconciler.update(trackId = 40, index = 5, dragging = false, animationsEnabled = true),
        )
    }

    @Test
    fun changed_track_animates_when_idle_but_reanchors_during_a_drag() {
        val reconciler = NowPlayingPositionReconciler()
        reconciler.update(trackId = 40, index = 2, dragging = false, animationsEnabled = true)

        assertEquals(
            NowPlayingPositionAction.ANIMATE,
            reconciler.update(trackId = 41, index = 3, dragging = false, animationsEnabled = true),
        )
        assertEquals(
            NowPlayingPositionAction.REANCHOR,
            reconciler.update(trackId = 42, index = 4, dragging = true, animationsEnabled = true),
        )
    }

    @Test
    fun animations_off_always_reseats_immediately() {
        val reconciler = NowPlayingPositionReconciler()
        reconciler.update(trackId = 40, index = 2, dragging = false, animationsEnabled = false)

        assertEquals(
            NowPlayingPositionAction.SNAP,
            reconciler.update(trackId = 41, index = 3, dragging = false, animationsEnabled = false),
        )
    }

    @Test
    fun a_commit_owned_settle_is_not_restarted_for_the_same_target_index() {
        val reconciler = NowPlayingPositionReconciler()
        reconciler.update(trackId = 40, index = 2, dragging = false, animationsEnabled = true)

        assertEquals(
            NowPlayingPositionAction.CONTINUE_SETTLE,
            reconciler.update(
                trackId = 41,
                index = 3,
                dragging = false,
                animationsEnabled = true,
                settlingTargetIndex = 3,
            ),
        )
        assertEquals(
            NowPlayingPositionAction.ANIMATE,
            reconciler.update(
                trackId = 42,
                index = 5,
                dragging = false,
                animationsEnabled = true,
                settlingTargetIndex = 4,
            ),
        )
    }
}
