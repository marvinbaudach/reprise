package de.reprise.spike

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class QueueFlashSlotTest {
    @Test
    fun noSlotFlashesWhenTheArrivalTintIsInactive() {
        assertNull(queueFlashSlot(flashing = false, offsetsHold = true, from = 2, to = 4))
    }

    @Test
    fun theComposedStartingSlotFlashesWhileTheOffsetsHold() {
        assertEquals(2, queueFlashSlot(flashing = true, offsetsHold = true, from = 2, to = 4))
    }

    @Test
    fun theComposedDestinationSlotFlashesAfterTheOffsetsRelease() {
        assertEquals(4, queueFlashSlot(flashing = true, offsetsHold = false, from = 2, to = 4))
    }

    @Test
    fun theSameSlotIsSelectedOnEitherSideOfAStationaryHandover() {
        assertEquals(3, queueFlashSlot(flashing = true, offsetsHold = true, from = 3, to = 3))
        assertEquals(3, queueFlashSlot(flashing = true, offsetsHold = false, from = 3, to = 3))
    }
}
