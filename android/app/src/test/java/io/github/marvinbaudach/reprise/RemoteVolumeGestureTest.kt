package io.github.marvinbaudach.reprise

import org.junit.Assert.assertEquals
import org.junit.Test

class RemoteVolumeGestureTest {
    @Test
    fun upThenDownWithinWindowSkipsForwardAndRestoresBeforeFirstTap() {
        val clock = FakeClock()
        val gesture = gesture(clock)

        assertEquals(stepUp, gesture.onAdjust(VolumeDirection.UP, 12))
        clock.advance(400)
        assertEquals(
            RemoteVolumeAction.Skip(VolumeDirection.UP, restoreVolume = 12),
            gesture.onAdjust(VolumeDirection.DOWN, 13),
        )
    }

    @Test
    fun downThenUpWithinWindowSkipsBackward() {
        val clock = FakeClock()
        val gesture = gesture(clock)

        assertEquals(stepDown, gesture.onAdjust(VolumeDirection.DOWN, 8))
        clock.advance(500)
        assertEquals(
            RemoteVolumeAction.Skip(VolumeDirection.DOWN, restoreVolume = 8),
            gesture.onAdjust(VolumeDirection.UP, 7),
        )
    }

    @Test
    fun sameDirectionTwiceRemainsTwoSteps() {
        val clock = FakeClock()
        val gesture = gesture(clock)

        assertEquals(stepUp, gesture.onAdjust(VolumeDirection.UP, 10))
        clock.advance(100)
        assertEquals(stepUp, gesture.onAdjust(VolumeDirection.UP, 11))
    }

    @Test
    fun oppositeDirectionAfterWindowRemainsTwoSteps() {
        val clock = FakeClock()
        val gesture = gesture(clock)

        assertEquals(stepUp, gesture.onAdjust(VolumeDirection.UP, 10))
        clock.advance(501)
        assertEquals(stepDown, gesture.onAdjust(VolumeDirection.DOWN, 11))
    }

    @Test
    fun foregroundAlwaysStepsAndClearsPendingTap() {
        val clock = FakeClock()
        var foreground = false
        val gesture = gesture(clock, isForeground = { foreground })

        assertEquals(stepUp, gesture.onAdjust(VolumeDirection.UP, 10))
        foreground = true
        clock.advance(100)
        assertEquals(stepDown, gesture.onAdjust(VolumeDirection.DOWN, 11))
        foreground = false
        clock.advance(100)
        assertEquals(stepUp, gesture.onAdjust(VolumeDirection.UP, 10))
    }

    @Test
    fun callbackAfterSkipStartsAFreshGesture() {
        val clock = FakeClock()
        val gesture = gesture(clock)

        assertEquals(stepUp, gesture.onAdjust(VolumeDirection.UP, 10))
        clock.advance(100)
        assertEquals(
            RemoteVolumeAction.Skip(VolumeDirection.UP, restoreVolume = 10),
            gesture.onAdjust(VolumeDirection.DOWN, 11),
        )
        clock.advance(100)
        assertEquals(stepUp, gesture.onAdjust(VolumeDirection.UP, 10))
    }

    @Test
    fun repeatTrainRemainsVolumeSteps() {
        val clock = FakeClock()
        val gesture = gesture(clock)

        repeat(5) { index ->
            if (index > 0) clock.advance(50)
            assertEquals(stepDown, gesture.onAdjust(VolumeDirection.DOWN, 12 - index))
        }
    }

    @Test
    fun restoreUsesVolumeBeforeFirstTapNotSecond() {
        val clock = FakeClock()
        val gesture = gesture(clock)

        assertEquals(stepDown, gesture.onAdjust(VolumeDirection.DOWN, 7))
        clock.advance(250)
        assertEquals(
            RemoteVolumeAction.Skip(VolumeDirection.DOWN, restoreVolume = 7),
            gesture.onAdjust(VolumeDirection.UP, 6),
        )
    }

    private fun gesture(
        clock: FakeClock,
        isForeground: () -> Boolean = { false },
    ) = RemoteVolumeGesture(
        rockMaxMs = 500,
        isForeground = isForeground,
        now = clock::now,
    )

    private companion object {
        val stepUp = RemoteVolumeAction.Step(VolumeDirection.UP)
        val stepDown = RemoteVolumeAction.Step(VolumeDirection.DOWN)
    }
}

private class FakeClock {
    private var timeMs = 0L

    fun now(): Long = timeMs

    fun advance(milliseconds: Long) {
        timeMs += milliseconds
    }
}
