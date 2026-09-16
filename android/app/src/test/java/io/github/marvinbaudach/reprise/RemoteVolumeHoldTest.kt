package io.github.marvinbaudach.reprise

import org.junit.Assert.assertEquals
import org.junit.Test

class RemoteVolumeHoldTest {
    @Test
    fun oneCallIsOneImmediateVolumeStep() {
        val clock = FakeClock()
        val hold = hold(clock)

        assertEquals(
            RemoteVolumeAction.Step(VolumeDirection.UP),
            hold.onAdjust(VolumeDirection.UP, currentVolume = 12),
        )
    }

    @Test
    fun measuredHoldRestoresBothLeadInStepsThenSwallowsUntilTheGapEnds() {
        val clock = FakeClock()
        val hold = hold(clock)

        assertEquals(stepUp, hold.onAdjust(VolumeDirection.UP, 12))
        clock.advance(250)
        assertEquals(stepUp, hold.onAdjust(VolumeDirection.UP, 13))
        clock.advance(50)
        assertEquals(
            RemoteVolumeAction.Skip(VolumeDirection.UP, restoreVolume = 12),
            hold.onAdjust(VolumeDirection.UP, 14),
        )
        repeat(2) {
            clock.advance(50)
            assertEquals(RemoteVolumeAction.Swallow, hold.onAdjust(VolumeDirection.UP, 12))
        }

        clock.advance(101)
        assertEquals(stepUp, hold.onAdjust(VolumeDirection.UP, 12))
    }

    @Test
    fun changingDirectionStartsAFreshPress() {
        val clock = FakeClock()
        val hold = hold(clock)

        assertEquals(stepUp, hold.onAdjust(VolumeDirection.UP, 12))
        clock.advance(50)
        assertEquals(stepDown, hold.onAdjust(VolumeDirection.DOWN, 13))
    }

    @Test
    fun foregroundRepeatsAlwaysRemainVolumeSteps() {
        val clock = FakeClock()
        val hold = hold(clock, isForeground = { true })

        repeat(4) { index ->
            if (index > 0) clock.advance(50)
            assertEquals(stepUp, hold.onAdjust(VolumeDirection.UP, 12 + index))
        }
    }

    @Test
    fun fastTripleTapNeverSkips() {
        val clock = FakeClock()
        val hold = hold(clock)

        repeat(3) { index ->
            if (index > 0) clock.advance(156)
            assertEquals(stepDown, hold.onAdjust(VolumeDirection.DOWN, 12 - index))
        }
    }

    @Test
    fun tapBeforeAHoldIsNotIncludedInTheRestore() {
        val clock = FakeClock()
        val hold = hold(clock)

        assertEquals(stepUp, hold.onAdjust(VolumeDirection.UP, 10))
        clock.advance(251)
        assertEquals(stepUp, hold.onAdjust(VolumeDirection.UP, 11))
        clock.advance(250)
        assertEquals(stepUp, hold.onAdjust(VolumeDirection.UP, 12))
        clock.advance(50)

        assertEquals(
            RemoteVolumeAction.Skip(VolumeDirection.UP, restoreVolume = 11),
            hold.onAdjust(VolumeDirection.UP, 13),
        )
    }

    @Test
    fun firstEverCallAndFastRepeatRestoreTheNullableFirstVolume() {
        val clock = FakeClock()
        val hold = hold(clock)

        assertEquals(stepDown, hold.onAdjust(VolumeDirection.DOWN, 7))
        clock.advance(50)

        assertEquals(
            RemoteVolumeAction.Skip(VolumeDirection.DOWN, restoreVolume = 7),
            hold.onAdjust(VolumeDirection.DOWN, 6),
        )
    }

    @Test
    fun stalePredecessorIsNeverUsedAsTheRestoreVolume() {
        val clock = FakeClock()
        val hold = hold(clock)

        assertEquals(stepUp, hold.onAdjust(VolumeDirection.UP, 3))
        clock.advance(501)
        assertEquals(stepUp, hold.onAdjust(VolumeDirection.UP, 4))
        clock.advance(50)

        assertEquals(
            RemoteVolumeAction.Skip(VolumeDirection.UP, restoreVolume = 4),
            hold.onAdjust(VolumeDirection.UP, 5),
        )
    }

    @Test
    fun repeatGapBoundaryIsInclusive() {
        assertSecondActionAfter(gapMs = 99, expected = RemoteVolumeAction.Skip(VolumeDirection.UP, 9))
        assertSecondActionAfter(gapMs = 100, expected = RemoteVolumeAction.Skip(VolumeDirection.UP, 9))
        assertSecondActionAfter(gapMs = 101, expected = stepUp)
    }

    private fun assertSecondActionAfter(gapMs: Long, expected: RemoteVolumeAction) {
        val clock = FakeClock()
        val hold = hold(clock)
        assertEquals(stepUp, hold.onAdjust(VolumeDirection.UP, 9))
        clock.advance(gapMs)
        assertEquals(expected, hold.onAdjust(VolumeDirection.UP, 10))
    }

    private fun hold(
        clock: FakeClock,
        isForeground: () -> Boolean = { false },
    ) = RemoteVolumeHold(
        repeatGapMaxMs = 100,
        leadInMaxMs = 500,
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
