package de.reprise.spike

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.animation.core.tween
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlin.math.min
import kotlin.math.roundToInt

/** How far the picked-up row grows while the finger holds it. */
internal const val QUEUE_DRAG_LIFT_SCALE = 1.02f

/** The shadow the lifted row casts over the rows it passes. */
internal const val QUEUE_DRAG_LIFT_ELEVATION_DP = 8

/** Picking a row up is quicker than putting it down. */
internal const val QUEUE_DRAG_LIFT_MS = 180

/** How long a neighbour takes to step out of the way. */
internal const val QUEUE_DRAG_NEIGHBOUR_MS = 280

/** How long the row takes to fall into the slot it was dropped on. */
internal const val QUEUE_DRAG_DROP_MS = 360

/** How long the moved row keeps its arrival tint. */
internal const val QUEUE_DRAG_FLASH_MS = 520

/** The strength of that tint at its brightest. */
internal const val QUEUE_DRAG_FLASH_ALPHA = 0.16f

/** How close to the top edge the finger has to come before the list follows. */
internal const val QUEUE_AUTOSCROLL_TOP_EDGE_DP = 90

/** The same at the bottom, where the mini player covers the last rows. */
internal const val QUEUE_AUTOSCROLL_BOTTOM_EDGE_DP = 180

/** The fastest the list is allowed to travel per frame. */
internal const val QUEUE_AUTOSCROLL_MAX_STEP_DP = 14

/** Turns "how far into the edge zone" into "how fast", linearly. */
private const val QUEUE_AUTOSCROLL_DIVISOR = 5f

/**
 * How long the drop keeps holding its offsets when the reloaded window has
 * not arrived yet. The reload is the normal end of the hold — see
 * [QueueReorderState.onOrderChanged]; this is only the backstop for a move
 * the core refused, so a failed edit cannot freeze the list askew.
 */
private const val QUEUE_RELOAD_GRACE_MS = 600L

/**
 * The decision behind [QueueReorderState.offsetsDescribe], as a plain
 * predicate so it can be pinned without a frame clock.
 *
 * Note what cannot be tested here, or anywhere in the unit suite: the defect
 * this guards against is a *timing* one, and `mainClock` erases it. With the
 * clock paused, `waitForIdle` drains the effect that releases the offsets
 * before any assertion can look, so the list is measured already correct —
 * the harness is friendlier than the device. The evidence for the fix is
 * therefore a measurement on hardware, recorded in the commit that added it.
 */
internal fun queueOffsetsDescribe(
    awaitingReload: Boolean,
    orderAtEdit: String?,
    order: String,
): Boolean = !awaitingReload || orderAtEdit == null || order == orderAtEdit

/**
 * Which composed slot reads the state-owned arrival tint.
 *
 * Before the reload, the moved row is still composed at [from] and only its
 * offset puts it over [to]. After the reload changes the row keys, the same
 * row is composed at [to]. Choosing by that handover instead of track id also
 * keeps duplicate occurrences of one track from flashing together.
 */
internal fun queueFlashSlot(
    flashing: Boolean,
    offsetsHold: Boolean,
    from: Int,
    to: Int,
): Int? = if (!flashing) null else if (offsetsHold) from else to

/** One curve for the whole gesture: fast out of the gate, long settle. */
internal val QueueDragEasing = CubicBezierEasing(0.2f, 0f, 0f, 1f)

/**
 * Which slot the dragged row would land on right now.
 *
 * The finger, not the list, decides: one row height of travel is one slot,
 * rounded, and the ends of the queue clamp it.
 */
internal fun queueDropTarget(
    startSlot: Int,
    dragPx: Float,
    rowHeightPx: Float,
    lastSlot: Int,
): Int {
    if (rowHeightPx <= 0f || lastSlot < 0) {
        return startSlot
    }
    return (startSlot + (dragPx / rowHeightPx).roundToInt()).coerceIn(0, lastSlot)
}

/**
 * How far a row that is *not* being dragged has to step aside, in row heights.
 *
 * The list is not recomputed while a finger is down — the rows between the
 * slot the drag started on and the slot it currently points at simply move one
 * place towards the gap the lifted row left behind. Everything else stays.
 */
internal fun queueNeighbourShiftRows(slot: Int, startSlot: Int, targetSlot: Int): Int = when {
    slot == startSlot -> 0
    targetSlot > startSlot && slot > startSlot && slot <= targetSlot -> -1
    targetSlot < startSlot && slot >= targetSlot && slot < startSlot -> 1
    else -> 0
}

/**
 * How far the list should travel this frame, in pixels, signed like
 * `LazyListState.scrollBy`: negative walks back towards the first row.
 *
 * Zero unless the finger is inside one of the two edge zones. The bottom zone
 * is the deeper one because the mini player sits in it, so the last rows are
 * behind glass rather than at the edge of the screen.
 */
internal fun queueAutoScrollStepPx(
    pointerYPx: Float,
    viewportTopPx: Float,
    viewportBottomPx: Float,
    topEdgePx: Float,
    bottomEdgePx: Float,
    maxStepPx: Float,
): Float {
    val intoTop = viewportTopPx + topEdgePx - pointerYPx
    if (intoTop > 0f) {
        return -min(maxStepPx, intoTop / QUEUE_AUTOSCROLL_DIVISOR)
    }
    val intoBottom = pointerYPx - (viewportBottomPx - bottomEdgePx)
    if (intoBottom > 0f) {
        return min(maxStepPx, intoBottom / QUEUE_AUTOSCROLL_DIVISOR)
    }
    return 0f
}

/**
 * What the list lends the drag so the queue can follow the finger past its own
 * edges. The grid layout hands over nothing, which simply means no auto-scroll.
 */
internal class QueueScrollPort(
    val viewportTopPx: () -> Float,
    val viewportBottomPx: () -> Float,
    val topEdgePx: () -> Float,
    val bottomEdgePx: () -> Float,
    val maxStepPx: () -> Float,
    /**
     * Moves the list and answers with what it actually took, which is zero at
     * either end. Deliberately the raw, non-suspending entry point a drag uses:
     * one frame is one step, and nothing can queue up behind a mutex while the
     * finger is still down.
     */
    val scrollBy: (Float) -> Float,
)

/**
 * The one drag a queue list can have in flight, and everything the rows read to
 * draw it.
 *
 * It lives above the rows rather than inside the handle because a reorder is
 * never about one row: the row under the finger is lifted and translated, and
 * every row between its old and its new slot steps aside. Both halves have to
 * agree on the same two numbers — where the drag started and where it points —
 * which is what this holds.
 *
 * The edit itself is deliberately the *last* thing that happens. The row is
 * animated into the slot first, `move` is called second, and the offsets are
 * only dropped once the reloaded window carries the new order. Calling `move`
 * on lift-off instead would put a reload, a re-key and a re-layout in the
 * middle of the animation, and the row would visibly snap back through its old
 * place on the way to its new one.
 */
@Stable
internal class QueueReorderState internal constructor(private val scope: CoroutineScope) {
    private enum class Phase { IDLE, DRAG, SETTLE }

    private var phase by mutableStateOf(Phase.IDLE)

    /** The slot the finger picked up, for as long as the drop is not finished. */
    var draggedSlot by mutableStateOf<Int?>(null)
        private set

    /** The slot that row would land on if the finger lifted now. */
    var targetSlot by mutableIntStateOf(0)
        private set

    /** The composed slot the current arrival tint started from. */
    var flashFrom by mutableIntStateOf(0)
        private set

    /** The composed slot the current arrival tint lands on after the reload. */
    var flashTo by mutableIntStateOf(0)
        private set

    /** The arrival tint outlives either row composition that reads it. */
    private val flash = Animatable(0f)

    val flashFraction: Float get() = flash.value

    /** Collaborators the composition re-supplies on every pass. */
    var haptics: QueueHaptics = QueueHaptics.None
    var move: ((Int, Long, Int) -> Unit)? = null
    var scrollPort: QueueScrollPort? = null

    /**
     * Whether the rows around the drag may step aside.
     *
     * A single column is the case the parting is drawn for: one slot down the
     * list is one row height down the screen. The wide-short layout tiles the
     * same rows into a grid, where that is no longer true, so there the row
     * still lifts and follows the finger but its neighbours hold still rather
     * than sliding to places they do not occupy.
     */
    var neighboursPart: Boolean = true

    private var trackId = 0L
    private var rowHeightPx = 0f
    private var lastSlot = 0
    private var pointerRootYPx = 0f
    private var fingerPx by mutableFloatStateOf(0f)
    private var scrolledPx by mutableFloatStateOf(0f)
    private val settlePx = Animatable(0f)
    private var awaitingReload by mutableStateOf(false)
    private var orderAtEdit by mutableStateOf<String?>(null)

    /**
     * The window order the rows were drawn from when the edit was sent.
     * Written every composition, read once at the drop.
     */
    var windowOrder: String = ""
    private var autoScroll: Job? = null

    /** Bumped per gesture so a finished drop cannot clean up its successor. */
    private var generation = 0L

    /** True exactly while a finger is down: what the lift envelope follows. */
    val lifted: Boolean get() = phase == Phase.DRAG

    /** Where the dragged row sits relative to its own slot, in pixels. */
    val translationPx: Float
        get() = when (phase) {
            Phase.IDLE -> 0f
            Phase.DRAG -> fingerPx + scrolledPx
            Phase.SETTLE -> settlePx.value
        }

    fun isDragging(slot: Int): Boolean = phase != Phase.IDLE && draggedSlot == slot

    /** How far [slot] steps aside, in row heights, while a drag is in flight. */
    fun neighbourShiftRows(slot: Int): Int {
        if (!neighboursPart) {
            return 0
        }
        val start = draggedSlot ?: return 0
        return queueNeighbourShiftRows(slot, start, targetSlot)
    }

    fun begin(
        slot: Int,
        trackId: Long,
        rowHeightPx: Float,
        slotCount: Int,
        pointerRootYPx: Float,
    ) {
        // A second finger on a second handle does not start a second reorder,
        // and a drop that is still settling keeps the list to itself.
        if (phase != Phase.IDLE) {
            return
        }
        generation += 1
        phase = Phase.DRAG
        draggedSlot = slot
        targetSlot = slot
        this.trackId = trackId
        this.rowHeightPx = rowHeightPx
        this.lastSlot = slotCount - 1
        this.pointerRootYPx = pointerRootYPx
        fingerPx = 0f
        scrolledPx = 0f
        haptics.lift()
        startAutoScroll()
    }

    fun dragBy(deltaPx: Float) {
        if (phase != Phase.DRAG) {
            return
        }
        fingerPx += deltaPx
        pointerRootYPx += deltaPx
        retarget()
    }

    fun end(cancelled: Boolean) {
        val start = draggedSlot ?: return
        if (phase != Phase.DRAG) {
            return
        }
        stopAutoScroll()
        val destination = if (cancelled) start else targetSlot
        targetSlot = destination
        phase = Phase.SETTLE
        val id = trackId
        val restingPx = (destination - start) * rowHeightPx
        val startedFrom = fingerPx + scrolledPx
        val myGeneration = generation
        scope.launch {
            if (destination == start) haptics.cancelled() else haptics.dropped()
            settlePx.snapTo(startedFrom)
            settlePx.animateTo(
                targetValue = restingPx,
                animationSpec = tween(QUEUE_DRAG_DROP_MS, easing = QueueDragEasing),
            )
            if (generation != myGeneration) {
                return@launch
            }
            if (destination == start) {
                release(myGeneration)
                return@launch
            }
            flashFrom = start
            flashTo = destination
            scope.launch {
                flash.snapTo(1f)
                flash.animateTo(0f, tween(QUEUE_DRAG_FLASH_MS, easing = QueueDragEasing))
            }
            // The row is standing in its new place already, so the edit is
            // invisible — which is the whole point of running it here.
            awaitingReload = true
            orderAtEdit = windowOrder
            move?.invoke(start, id, destination)
            delay(QUEUE_RELOAD_GRACE_MS)
            release(myGeneration)
        }
    }

    /**
     * Whether the offsets still describe the list the rows are drawn from.
     *
     * They stop describing it the instant the edit comes back. The reloaded
     * window already carries the new order, so a parting offset laid on top of
     * it displaces the row a second time: it covers the neighbour above and
     * leaves its own slot standing empty, which reads as the two rows swapping
     * once more after the drop.
     *
     * [onOrderChanged] releases the offsets for the same reason, but it is a
     * coroutine and therefore runs *after* the composition that has already
     * drawn the new order — measured on a Pixel 10 Pro XL, three frames of
     * exactly that double count. This is the same question asked during
     * composition, where the answer is still in time to be used.
     */
    fun offsetsDescribe(order: String): Boolean =
        queueOffsetsDescribe(awaitingReload, orderAtEdit, order)

    /**
     * Called when the window the rows are drawn from has changed order, which
     * for a queue means the edit came back. The offsets have done their job the
     * moment the list itself agrees with them.
     */
    fun onOrderChanged() {
        if (awaitingReload) {
            release(generation)
        }
    }

    private fun release(forGeneration: Long) {
        if (generation != forGeneration || phase == Phase.IDLE) {
            return
        }
        phase = Phase.IDLE
        draggedSlot = null
        fingerPx = 0f
        scrolledPx = 0f
        awaitingReload = false
        orderAtEdit = null
    }

    private fun retarget() {
        val start = draggedSlot ?: return
        val next = queueDropTarget(start, fingerPx + scrolledPx, rowHeightPx, lastSlot)
        if (next != targetSlot) {
            targetSlot = next
            haptics.crossedBoundary()
        }
    }

    /**
     * One frame, one step, for as long as the finger is down.
     *
     * The loop deliberately does not stop when the finger leaves an edge zone
     * or when the list runs out of room: the finger can hold perfectly still
     * inside the zone for a second and expect the queue to keep coming, and a
     * loop that ended on the first idle frame would have nothing left to
     * restart it. It costs a frame callback for exactly as long as a gesture
     * that is already repainting every frame anyway.
     */
    private fun startAutoScroll() {
        val port = scrollPort ?: return
        val myGeneration = generation
        autoScroll = scope.launch {
            while (isActive && generation == myGeneration && phase == Phase.DRAG) {
                withFrameNanos { }
                if (phase != Phase.DRAG) {
                    break
                }
                val step = stepFor(port)
                if (step == 0f) {
                    continue
                }
                val consumed = port.scrollBy(step)
                if (consumed == 0f) {
                    continue
                }
                // The row keeps its layout slot while the list moves under it,
                // so the travelled distance counts towards the drag exactly as
                // the finger's own does — otherwise the target index drifts.
                scrolledPx += consumed
                retarget()
            }
        }
    }

    private fun stopAutoScroll() {
        autoScroll?.cancel()
        autoScroll = null
    }

    private fun stepFor(port: QueueScrollPort): Float = queueAutoScrollStepPx(
        pointerYPx = pointerRootYPx,
        viewportTopPx = port.viewportTopPx(),
        viewportBottomPx = port.viewportBottomPx(),
        topEdgePx = port.topEdgePx(),
        bottomEdgePx = port.bottomEdgePx(),
        maxStepPx = port.maxStepPx(),
    )
}

@Composable
internal fun rememberQueueReorderState(): QueueReorderState {
    val scope = rememberCoroutineScope()
    return remember(scope) { QueueReorderState(scope) }
}
