package io.github.marvinbaudach.reprise

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Whether a drag's offsets still describe the list the rows are drawn from.
 *
 * They are laid over a window, and the moment the edit comes back that window
 * is a different one: it carries the move already. Applying them on top of it
 * moves the row a second time, over the row above and out of its own slot.
 */
class QueueOffsetsDescribeTest {
    private val beforeTheEdit = "a,b,c"
    private val afterTheEdit = "a,c,b"

    @Test
    fun offsetsDescribeAListNoEditHasTouched() {
        assertTrue(queueOffsetsDescribe(awaitingReload = false, orderAtEdit = null, order = beforeTheEdit))
        assertTrue(queueOffsetsDescribe(awaitingReload = false, orderAtEdit = beforeTheEdit, order = afterTheEdit))
    }

    @Test
    fun offsetsStillDescribeTheWindowTheEditWasSentAgainst() {
        assertTrue(
            "the reload has not arrived yet, so the offsets are all that holds the row in place",
            queueOffsetsDescribe(awaitingReload = true, orderAtEdit = beforeTheEdit, order = beforeTheEdit),
        )
    }

    @Test
    fun offsetsStopDescribingTheWindowTheEditCameBackIn() {
        assertFalse(
            "the reloaded window carries the move; the offsets would move the row twice",
            queueOffsetsDescribe(awaitingReload = true, orderAtEdit = beforeTheEdit, order = afterTheEdit),
        )
    }

    @Test
    fun anEditWhoseWindowWasNeverRecordedLeavesTheOffsetsAlone() {
        // A drop onto the starting slot writes nothing and records nothing.
        assertTrue(queueOffsetsDescribe(awaitingReload = true, orderAtEdit = null, order = afterTheEdit))
    }
}
