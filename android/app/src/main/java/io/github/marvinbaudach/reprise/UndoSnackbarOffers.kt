package io.github.marvinbaudach.reprise

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue

/** How long an undo stays on offer when nothing asks for longer. */
internal const val UNDO_WINDOW_MS = 6_000L

internal const val UNDO_ACTION_LABEL = "Undo"

/**
 * One thing the listener may still take back.
 *
 * [onExpire] runs when the window passes untouched; [onSuperseded] when a newer
 * offer takes the slot first. They differ for a deferred delete only in
 * timing — both mean "no undo any more, carry on" — but they stay two callbacks
 * because the slot, not the offer, decides which of the two happened.
 *
 * An offer that [holdsSlot] keeps its window to the end against offers that do
 * not: they wait their turn instead of ending it.
 */
internal class UndoOffer(
    val token: Long,
    val message: String,
    val windowMs: Long,
    val holdsSlot: Boolean,
    val onUndo: () -> Unit,
    val onExpire: () -> Unit,
    val onSuperseded: () -> Unit,
)

/**
 * The single undo slot of the library surface.
 *
 * Deferred deletes and queue removals share it: the snackbar shows one line at
 * a time. A second offer ends the first one's chance, exactly as a second
 * delete commits the first — except that a queue removal never ends a delete's
 * window early. It waits until the delete's offer is over and is shown then,
 * with a window of its own; a newer offer drops it.
 *
 * The window is timed here, on the injected [scheduleAfter], and not by the
 * snackbar: Material has no 6 s duration, and a snackbar's timer dies with the
 * composition it was shown from — a rotation would otherwise take the undo and
 * the deadline with it.
 *
 * Nothing here is persisted. A process that dies inside the window leaves the
 * offer — and whatever it was holding back — unexecuted, which is the safe
 * direction for a delete.
 */
internal class UndoOffers(
    private val scheduleAfter: (Long, () -> Unit) -> Unit,
) {
    var current by mutableStateOf<UndoOffer?>(null)
        private set
    private var waiting: UndoOffer? = null
    private var lastToken = 0L

    /** Runs [block] once, [delayMs] from now, on the thread offers are used on. */
    fun after(delayMs: Long, block: () -> Unit) = scheduleAfter(delayMs, block)

    fun show(
        message: String,
        onUndo: () -> Unit,
        windowMs: Long = UNDO_WINDOW_MS,
        holdsSlot: Boolean = false,
        onExpire: () -> Unit = {},
        onSuperseded: () -> Unit = onExpire,
    ) {
        val offer = UndoOffer(++lastToken, message, windowMs, holdsSlot, onUndo, onExpire, onSuperseded)
        val previous = current
        if (previous != null && previous.holdsSlot && !holdsSlot) {
            val displaced = waiting
            waiting = offer
            displaced?.onSuperseded?.invoke()
            return
        }
        val dropped = waiting
        waiting = null
        present(offer)
        previous?.onSuperseded?.invoke()
        dropped?.onSuperseded?.invoke()
    }

    /** The listener pressed Undo on the offer with [token]; a stale press is ignored. */
    fun undo(token: Long) {
        val offer = current?.takeIf { it.token == token } ?: return
        current = null
        offer.onUndo()
        promoteWaiting()
    }

    private fun expire(token: Long) {
        val offer = current?.takeIf { it.token == token } ?: return
        current = null
        offer.onExpire()
        promoteWaiting()
    }

    private fun present(offer: UndoOffer) {
        current = offer
        scheduleAfter(offer.windowMs) { expire(offer.token) }
    }

    private fun promoteWaiting() {
        val next = waiting ?: return
        waiting = null
        present(next)
    }

    /** Drops the offers without running anything: the screen is gone. */
    fun discard() {
        current = null
        waiting = null
    }
}
