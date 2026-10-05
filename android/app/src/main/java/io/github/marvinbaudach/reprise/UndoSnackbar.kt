package io.github.marvinbaudach.reprise

import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.SnackbarDuration
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.SnackbarResult
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

/** How long an undo stays on offer. */
internal const val UNDO_WINDOW_MS = 6_000L

internal const val UNDO_ACTION_LABEL = "Undo"

private val SNACKBAR_GAP = 8.dp

/**
 * One thing the listener may still take back.
 *
 * [onExpire] runs when the window passes untouched; [onSuperseded] when a newer
 * offer takes the slot first. They differ for a deferred delete only in
 * timing — both mean "no undo any more, carry on" — but they stay two callbacks
 * because the slot, not the offer, decides which of the two happened.
 */
internal class UndoOffer(
    val token: Long,
    val message: String,
    val onUndo: () -> Unit,
    val onExpire: () -> Unit,
    val onSuperseded: () -> Unit,
)

/**
 * The single undo slot of the library surface.
 *
 * Deferred deletes and queue removals share it: the snackbar shows one line at
 * a time, and a second offer ends the first one's chance, exactly as a second
 * delete commits the first. The window is timed here, on the injected
 * [scheduleAfter], and not by the snackbar: Material has no 6 s duration, and a
 * snackbar's timer dies with the composition it was shown from — a rotation
 * would otherwise take the undo and the deadline with it.
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
    private var lastToken = 0L

    fun show(
        message: String,
        onUndo: () -> Unit,
        onExpire: () -> Unit = {},
        onSuperseded: () -> Unit = onExpire,
    ) {
        val previous = current
        val offer = UndoOffer(++lastToken, message, onUndo, onExpire, onSuperseded)
        current = offer
        scheduleAfter(UNDO_WINDOW_MS) { expire(offer.token) }
        previous?.onSuperseded?.invoke()
    }

    /** The listener pressed Undo on the offer with [token]; a stale press is ignored. */
    fun undo(token: Long) {
        val offer = current?.takeIf { it.token == token } ?: return
        current = null
        offer.onUndo()
    }

    private fun expire(token: Long) {
        val offer = current?.takeIf { it.token == token } ?: return
        current = null
        offer.onExpire()
    }

    /** Drops the offer without running anything: the screen is gone. */
    fun discard() {
        current = null
    }
}

/**
 * The library's one snackbar host, drawn over the whole library surface.
 *
 * It floats in the overlay layer and has no layout height, so showing or
 * dismissing it never reflows the list. [bottomInset] is the height of what
 * the library keeps along its bottom edge (mini player and navigation bar);
 * with none — the dock — the system navigation inset stands in.
 */
@Composable
internal fun BoxScope.UndoSnackbarHost(deletions: PendingDeletions, bottomInset: Dp) {
    val offers = deletions.offers
    val hostState = remember { SnackbarHostState() }
    // The delete that fires when the window passes needs the transport of the
    // activity that is on screen then, which is not the one that started it
    // after a rotation. This host is always composed with the library, so it
    // is the one place that sees every activity come and go.
    val controls = LocalPlaybackControls.current
    DisposableEffect(deletions, controls) {
        deletions.bind(controls)
        onDispose { deletions.unbind(controls) }
    }
    val offer = offers.current
    LaunchedEffect(offer?.token) {
        if (offer == null) return@LaunchedEffect
        // Indefinite on purpose: the offer's own timer ends it, and a restarted
        // effect (a new offer, or none) cancels this call, which dismisses it.
        val result = hostState.showSnackbar(
            message = offer.message,
            actionLabel = UNDO_ACTION_LABEL,
            duration = SnackbarDuration.Indefinite,
        )
        if (result == SnackbarResult.ActionPerformed) offers.undo(offer.token)
    }
    SnackbarHost(
        hostState = hostState,
        modifier = Modifier
            .align(Alignment.BottomCenter)
            .then(if (bottomInset == 0.dp) Modifier.navigationBarsPadding() else Modifier)
            .padding(bottom = bottomInset + SNACKBAR_GAP, start = SNACKBAR_GAP, end = SNACKBAR_GAP)
            .testTag("undo-snackbar-host"),
    )
}
