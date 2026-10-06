package io.github.marvinbaudach.reprise

import android.content.Context
import android.os.Build
import android.view.accessibility.AccessibilityManager
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Snackbar
import androidx.compose.material3.SnackbarDefaults
import androidx.compose.material3.SnackbarDuration
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.SnackbarVisuals
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner

private val SNACKBAR_GAP = 8.dp

/**
 * The offer's window as the listener's accessibility settings want it: the
 * platform lengthens a timed control for people who need longer to read and
 * reach it. Older platforms answer with [UNDO_WINDOW_MS] unchanged.
 */
internal fun undoWindowFor(manager: AccessibilityManager?): Long {
    if (manager == null || Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) return UNDO_WINDOW_MS
    return manager.getRecommendedTimeoutMillis(
        UNDO_WINDOW_MS.toInt(),
        AccessibilityManager.FLAG_CONTENT_CONTROLS or AccessibilityManager.FLAG_CONTENT_TEXT,
    ).toLong()
}

private fun Context.undoWindow(): Long =
    undoWindowFor(getSystemService(AccessibilityManager::class.java))

/** What the snackbar shows, and which offer it answers for. */
private class UndoVisuals(val token: Long, override val message: String) : SnackbarVisuals {
    override val actionLabel: String = UNDO_ACTION_LABEL
    override val withDismissAction: Boolean = false
    // Indefinite on purpose: the offer's own timer ends it, and a restarted
    // effect (a new offer, or none) cancels the call, which dismisses it.
    override val duration: SnackbarDuration = SnackbarDuration.Indefinite
}

/**
 * The library's one snackbar host, drawn over the whole library surface.
 *
 * It floats in the overlay layer and has no layout height, so showing or
 * dismissing it never reflows the list. [bottomClearance] is how far above the
 * bottom edge it floats — see [undoSnackbarClearance]; it is read while the host
 * is placed, so a change never recomposes the library. A clearance of zero —
 * nothing kept along the bottom edge — falls back to the system navigation inset.
 */
@Composable
internal fun BoxScope.UndoSnackbarHost(deletions: PendingDeletions, bottomClearance: () -> Dp) {
    val offers = deletions.offers
    val hostState = remember { SnackbarHostState() }
    val context = LocalContext.current
    // The delete that fires when the window passes needs the transport of the
    // activity that is on screen then, which is not the one that started it
    // after a rotation. This host is always composed with the library, so it
    // is the one place that sees every activity come and go.
    val controls = LocalPlaybackControls.current
    DisposableEffect(deletions, controls) {
        deletions.bind(controls)
        onDispose { deletions.unbind(controls) }
    }
    DisposableEffect(deletions, context) {
        deletions.undoWindowMs = { context.undoWindow() }
        onDispose { deletions.undoWindowMs = { UNDO_WINDOW_MS } }
    }
    // A window that is not on screen cannot be asked to delete anything: its
    // service is unbound, and a delete would be refused and its rows put back.
    // Whatever expires meanwhile waits for the window to return.
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    DisposableEffect(deletions, lifecycle) {
        deletions.setForeground(lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED))
        val observer = LifecycleEventObserver { _, event ->
            when (event) {
                Lifecycle.Event.ON_STOP -> deletions.setForeground(false)
                Lifecycle.Event.ON_RESUME -> deletions.setForeground(true)
                else -> Unit
            }
        }
        lifecycle.addObserver(observer)
        onDispose { lifecycle.removeObserver(observer) }
    }
    val offer = offers.current
    LaunchedEffect(offer?.token) {
        if (offer == null) return@LaunchedEffect
        hostState.showSnackbar(UndoVisuals(offer.token, offer.message))
    }
    val navigationBars = WindowInsets.navigationBars
    SnackbarHost(
        hostState = hostState,
        modifier = Modifier
            .align(Alignment.BottomCenter)
            .padding(horizontal = SNACKBAR_GAP)
            .offset {
                val clearance = bottomClearance()
                val bottom = if (clearance == 0.dp) navigationBars.getBottom(this) else clearance.roundToPx()
                IntOffset(0, -(bottom + SNACKBAR_GAP.roundToPx()))
            }
            .testTag("undo-snackbar-host"),
    ) { data ->
        val token = (data.visuals as UndoVisuals).token
        Snackbar(
            action = {
                // Undo is decided here, in the tap, and not by the host's
                // result: that is resumed on the compose dispatcher, a frame
                // after the tap, and a window that ends in between would win.
                TextButton(
                    onClick = {
                        offers.undo(token)
                        data.dismiss()
                    },
                ) {
                    Text(UNDO_ACTION_LABEL, color = SnackbarDefaults.actionColor)
                }
            },
        ) {
            Text(data.visuals.message)
        }
    }
}
