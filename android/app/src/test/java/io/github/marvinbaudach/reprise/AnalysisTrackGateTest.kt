package io.github.marvinbaudach.reprise

import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class AnalysisTrackGateTest {
    @Test
    fun nav_15e_a_supersede_for_a_track_that_lost_its_place_never_runs() {
        val gate = AnalysisTrackGate()
        val superseded = mutableListOf<Long>()
        gate.moveTo(2)
        gate.moveTo(3)

        gate.supersedeOthers(keepTrackId = 2) { superseded += it }
        gate.supersedeOthers(keepTrackId = 3) { superseded += it }

        assertEquals(listOf(3L), superseded)
    }

    @Test
    fun nav_15e_stopping_after_a_switch_still_supersedes_the_outgoing_track() {
        val gate = AnalysisTrackGate()
        val superseded = mutableListOf<Long>()
        gate.moveTo(1)
        gate.moveTo(2)
        gate.moveTo(null)

        gate.supersedeOthers(keepTrackId = 2) { superseded += it }

        assertEquals(
            "a stop after the switch must not leave the outgoing decode running",
            listOf(2L),
            superseded,
        )
    }

    @Test
    fun nav_15e_the_track_cannot_move_while_a_supersede_is_in_the_library() {
        val gate = AnalysisTrackGate()
        gate.moveTo(2)
        val inLibrary = CountDownLatch(1)
        val leaveLibrary = CountDownLatch(1)
        val moved = CountDownLatch(1)
        val supersede = Thread {
            gate.supersedeOthers(keepTrackId = 2) {
                inLibrary.countDown()
                leaveLibrary.await(TEST_TIMEOUT_SECONDS, TimeUnit.SECONDS)
            }
        }.apply { start() }
        assertTrue("the supersede never reached the library", inLibrary.await(TEST_TIMEOUT_SECONDS, TimeUnit.SECONDS))

        val mover = Thread {
            gate.moveTo(3)
            moved.countDown()
        }.apply { start() }

        assertFalse(
            "the track moved to C while the call that keeps B was cancelling everything else",
            moved.await(BLOCKED_PROOF_MS, TimeUnit.MILLISECONDS),
        )
        leaveLibrary.countDown()
        assertTrue("the track never moved", moved.await(TEST_TIMEOUT_SECONDS, TimeUnit.SECONDS))
        supersede.join()
        mover.join()
        assertEquals(3L, gate.current)
    }

    private companion object {
        const val TEST_TIMEOUT_SECONDS = 2L
        const val BLOCKED_PROOF_MS = 200L
    }
}
