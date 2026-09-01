package io.github.marvinbaudach.reprise

import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * The arithmetic behind the drag, on its own.
 *
 * These three functions are what makes the gesture feel like the row is being
 * carried rather than computed: which slot the finger currently points at, how
 * far the rows around it stand aside, and how hard the list pulls when the
 * finger reaches an edge. They are pure so the answers can be checked without
 * a frame clock in the way.
 */
class QueueReorderTest {
    @Test
    fun halfARowOfTravelStillPointsAtTheSlotItStartedOn() {
        assertEquals(3, queueDropTarget(startSlot = 3, dragPx = 20f, rowHeightPx = 72f, lastSlot = 9))
        assertEquals(3, queueDropTarget(startSlot = 3, dragPx = -20f, rowHeightPx = 72f, lastSlot = 9))
        assertEquals(4, queueDropTarget(startSlot = 3, dragPx = 40f, rowHeightPx = 72f, lastSlot = 9))
        assertEquals(2, queueDropTarget(startSlot = 3, dragPx = -40f, rowHeightPx = 72f, lastSlot = 9))
    }

    @Test
    fun theEndsOfTheQueueStopTheTarget() {
        assertEquals(9, queueDropTarget(startSlot = 3, dragPx = 5_000f, rowHeightPx = 72f, lastSlot = 9))
        assertEquals(0, queueDropTarget(startSlot = 3, dragPx = -5_000f, rowHeightPx = 72f, lastSlot = 9))
    }

    @Test
    fun aRowHeightOfZeroLeavesTheTargetWhereItWas() {
        // The first composition measures nothing; a division by it would put
        // the row at the far end of the queue before the finger has moved.
        assertEquals(3, queueDropTarget(startSlot = 3, dragPx = 400f, rowHeightPx = 0f, lastSlot = 9))
    }

    @Test
    fun onlyTheRowsBetweenTheOldAndTheNewSlotStepAside() {
        // Dragging slot 1 down to slot 3: 2 and 3 move up one place, 0 and 4
        // never learn about it, and the dragged row is carried, not shifted.
        val shifts = (0..4).map { queueNeighbourShiftRows(it, startSlot = 1, targetSlot = 3) }
        assertEquals(listOf(0, 0, -1, -1, 0), shifts)
    }

    @Test
    fun draggingUpwardsPushesTheSameRangeTheOtherWay() {
        val shifts = (0..4).map { queueNeighbourShiftRows(it, startSlot = 3, targetSlot = 1) }
        assertEquals(listOf(0, 1, 1, 0, 0), shifts)
    }

    @Test
    fun aDragThatPointsBackAtItsOwnSlotMovesNobody() {
        val shifts = (0..4).map { queueNeighbourShiftRows(it, startSlot = 2, targetSlot = 2) }
        assertEquals(listOf(0, 0, 0, 0, 0), shifts)
    }

    @Test
    fun theListHoldsStillWhileTheFingerIsAwayFromBothEdges() {
        assertEquals(0f, step(pointerYPx = 500f), 0.001f)
    }

    @Test
    fun theEdgeZonesPullProportionallyAndCapOut() {
        // 40 px into the top zone at a fifth of the distance per frame, and
        // negative because the list has to walk back towards the first row.
        assertEquals(-8f, step(pointerYPx = 150f), 0.001f)
        // Deep into the zone the step is capped rather than teleporting.
        assertEquals(-14f, step(pointerYPx = 0f), 0.001f)
        // The bottom zone is the deeper one: the mini player sits inside it.
        assertEquals(8f, step(pointerYPx = 860f), 0.001f)
        assertEquals(14f, step(pointerYPx = 1_000f), 0.001f)
    }

    private fun step(pointerYPx: Float): Float = queueAutoScrollStepPx(
        pointerYPx = pointerYPx,
        viewportTopPx = 100f,
        viewportBottomPx = 1_000f,
        topEdgePx = 90f,
        bottomEdgePx = 180f,
        maxStepPx = 14f,
    )
}
