package io.github.marvinbaudach.reprise

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class QueueFlashSlotTest {
    @Test
    fun noSlotFlashesWhenTheArrivalTintIsInactive() {
        assertNull(queueFlashSlot(flashing = false, handedOver = false, from = 2, to = 4))
    }

    @Test
    fun theComposedStartingSlotFlashesBeforeTheOrderChanges() {
        assertEquals(2, queueFlashSlot(flashing = true, handedOver = false, from = 2, to = 4))
    }

    @Test
    fun theComposedDestinationSlotFlashesAfterTheOrderChanges() {
        assertEquals(4, queueFlashSlot(flashing = true, handedOver = true, from = 2, to = 4))
    }

    @Test
    fun theSameSlotIsSelectedOnEitherSideOfAStationaryHandover() {
        assertEquals(3, queueFlashSlot(flashing = true, handedOver = false, from = 3, to = 3))
        assertEquals(3, queueFlashSlot(flashing = true, handedOver = true, from = 3, to = 3))
    }

    @Test
    fun handoverStaysAtTheDestinationWhenIdleOffsetsMayApplyAgain() {
        val beforeTheEdit = "queue-0-a,queue-1-b"
        val afterTheEdit = "queue-0-b,queue-1-a"

        assertTrue(
            queueOffsetsDescribe(
                awaitingReload = false,
                orderAtEdit = beforeTheEdit,
                order = afterTheEdit,
            ),
        )
        assertEquals(1, queueFlashSlot(flashing = true, handedOver = true, from = 0, to = 1))
    }
}
