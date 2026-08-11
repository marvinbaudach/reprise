package de.reprise.spike

import android.util.Log
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.size
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.IconButton
import androidx.compose.material3.TextButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.composed
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.DpOffset
import androidx.compose.ui.unit.dp
import uniffi.reprise_android_ffi.AndroidTrashReport

@Stable
internal class TrackContextMenuAnchorState {
    var expanded by mutableStateOf(false)
    var touchOffset by mutableStateOf(DpOffset.Zero)

    /**
     * What the last chosen menu item answered. It lives here rather than inside
     * [TrackContextMenu] because the row, not the menu, owns the place it can
     * be read: see [TrackContextMenuMessage].
     */
    var message by mutableStateOf<TransientMessage?>(null)
    internal var heightPx = 0

    fun say(text: String) {
        message = TransientMessage(text).after(message)
    }
}

@Composable
internal fun rememberTrackContextMenuAnchorState() = remember { TrackContextMenuAnchorState() }

/**
 * Renders the acknowledgement [anchor] is holding, in a slot of the caller's own.
 *
 * It is a separate composable because neither place inside a row works. Dropped
 * beside the row's content, it lands at TopStart of that `Box` — on top of the
 * cover and the title, inside a clipped 72 dp `Surface`. Moved into a `Column`
 * together with the menu, it would displace the `DropdownMenu`'s placeholder,
 * and a popup is anchored by exactly where its placeholder sits.
 *
 * So the row calls this below its own content, which is the shape
 * [FavouriteHeartButton] already uses for its failure: control first, message
 * in a slot underneath. That matters beyond tidiness — a partial deletion
 * ("2 of 12 could not be deleted") is reported through this same text.
 */
@Composable
internal fun TrackContextMenuMessage(anchor: TrackContextMenuAnchorState) {
    TransientMessageText(anchor.message) { anchor.message = null }
}

@OptIn(ExperimentalFoundationApi::class)
internal fun Modifier.trackContextMenuAnchor(
    state: TrackContextMenuAnchorState,
    onClick: () -> Unit,
): Modifier = composed {
    val density = LocalDensity.current
    val haptic = LocalHapticFeedback.current
    onSizeChanged { size -> state.heightPx = size.height }
        .pointerInput(state, density) {
            awaitEachGesture {
                val down = awaitFirstDown(requireUnconsumed = false, pass = PointerEventPass.Initial)
                state.touchOffset = with(density) {
                    DpOffset(
                        x = down.position.x.toDp(),
                        y = (down.position.y - state.heightPx).toDp(),
                    )
                }
            }
        }
        .combinedClickable(
            onClick = onClick,
            onLongClick = {
                haptic.performHapticFeedback(HapticFeedbackType.LongPress)
                state.expanded = true
            },
        )
}

internal data class LibraryTrackMenuTarget(
    val label: String,
    val trackCount: Long,
    val resolveTrackIds: () -> List<Long>,
    val play: (List<Long>) -> Unit,
)

internal data class QueueTrackMenuTarget(
    val trackId: Long,
    val position: Int,
    val rowCount: Int,
    val actions: QueueRowActions,
)

private data class TrackDeletionTarget(
    val label: String,
    val trackCount: Long,
    val resolveTrackIds: () -> List<Long>,
)

private fun LibraryTrackMenuTarget.deletionTarget() = TrackDeletionTarget(
    label = label,
    trackCount = trackCount,
    resolveTrackIds = resolveTrackIds,
)

@Composable
internal fun TrackContextMenu(
    anchor: TrackContextMenuAnchorState,
    target: QueueTrackMenuTarget,
) {
    DropdownMenu(
        expanded = anchor.expanded,
        onDismissRequest = { anchor.expanded = false },
        offset = anchor.touchOffset,
    ) {
        DropdownMenuItem(
            text = { Text("Play now") },
            onClick = {
                anchor.expanded = false
                target.actions.play(target.position, target.trackId)
            },
        )
        DropdownMenuItem(
            text = { Text("Move up") },
            enabled = target.position > 0,
            onClick = {
                anchor.expanded = false
                target.actions.move(target.position, target.trackId, target.position - 1)
            },
        )
        DropdownMenuItem(
            text = { Text("Move down") },
            enabled = target.position + 1 < target.rowCount,
            onClick = {
                anchor.expanded = false
                target.actions.move(target.position, target.trackId, target.position + 1)
            },
        )
        DropdownMenuItem(
            text = { Text("Remove from queue") },
            onClick = {
                anchor.expanded = false
                target.actions.remove(target.position, target.trackId)
            },
        )
    }
}

@Composable
internal fun TrackContextMenu(
    anchor: TrackContextMenuAnchorState,
    target: LibraryTrackMenuTarget,
) {
    val controls = LocalPlaybackControls.current
    var deleteConfirmation by remember { mutableStateOf<TrackDeletionTarget?>(null) }

    fun resolvedIds(): List<Long>? = runCatching(target.resolveTrackIds)
        .onFailure { error ->
            anchor.say("Could not load the tracks: ${error.message ?: "unknown error"}")
        }
        .getOrNull()

    fun queued(outcome: Result<UInt>) {
        val text = outcome.fold(
            onSuccess = { count ->
                if (count == 0u) {
                    "No tracks were queued."
                } else {
                    "$count ${if (count == 1u) "track" else "tracks"} queued"
                }
            },
            onFailure = { error ->
                "Could not edit the queue: ${error.message ?: "unknown error"}"
            },
        )
        anchor.say(text)
    }

    DropdownMenu(
        expanded = anchor.expanded,
        onDismissRequest = { anchor.expanded = false },
        offset = anchor.touchOffset,
    ) {
        DropdownMenuItem(
            text = { Text("Play") },
            onClick = {
                anchor.expanded = false
                resolvedIds()?.let(target.play)
            },
        )
        DropdownMenuItem(
            text = { Text("Play next") },
            onClick = {
                anchor.expanded = false
                resolvedIds()?.let { ids -> controls.queueTracksNext(ids, ::queued) }
            },
        )
        DropdownMenuItem(
            text = { Text("Add to queue") },
            onClick = {
                anchor.expanded = false
                resolvedIds()?.let { ids -> controls.queueTracksLast(ids, ::queued) }
            },
        )
        HorizontalDivider()
        DropdownMenuItem(
            text = { Text("Delete from device…") },
            onClick = {
                anchor.expanded = false
                deleteConfirmation = target.deletionTarget()
            },
        )
    }
    TrackDeletionConfirmation(
        target = deleteConfirmation,
        dismiss = { deleteConfirmation = null },
        report = anchor::say,
    )
}

@Composable
internal fun NowPlayingTrackContextMenu(track: LibraryTrack) {
    var expanded by remember { mutableStateOf(false) }
    var deleteConfirmation by remember { mutableStateOf<TrackDeletionTarget?>(null) }
    var message by remember { mutableStateOf<TransientMessage?>(null) }
    val target = remember(track.id, track.title) {
        TrackDeletionTarget(track.title, 1) { listOf(track.id) }
    }
    // The message needs a slot of its own, exactly as in FavouriteHeartButton
    // next door: as a bare sibling it becomes another cell of the actions Row
    // and squeezes the controls sideways.
    Column {
        Box {
            IconButton(
                onClick = { expanded = true },
                modifier = Modifier.size(48.dp).testTag("now-playing-overflow"),
            ) {
                MaterialSymbol("more_vert", "More actions")
            }
            DropdownMenu(expanded = expanded, onDismissRequest = { expanded = false }) {
                DropdownMenuItem(
                    text = { Text("Delete from device…") },
                    onClick = {
                        expanded = false
                        deleteConfirmation = target
                    },
                )
            }
        }
        TrackDeletionConfirmation(
            target = deleteConfirmation,
            dismiss = { deleteConfirmation = null },
            report = { text -> message = TransientMessage(text).after(message) },
        )
        TransientMessageText(message) { message = null }
    }
}

private const val TRACK_MENU_TAG = "TrackContextMenu"

/**
 * What a finished deletion says to a person.
 *
 * The count is the honest part and stays: a partial deletion is reported as a
 * partial deletion, never as success. What does not belong on the screen is the
 * reason each file gave — rusqlite and SAF phrase those for a developer
 * ("Os { code: 13, kind: PermissionDenied … }"), and one line of a 72 dp row is
 * the wrong place to read them. [logTrashFailures] keeps them.
 */
internal fun trashOutcomeMessage(report: AndroidTrashReport, requested: Int): String =
    if (report.failures.isEmpty()) {
        val deleted = report.removedIds.size
        "$deleted ${if (deleted == 1) "track" else "tracks"} deleted"
    } else {
        "${report.failures.size} of $requested could not be deleted"
    }

/** Keeps the per-file detail the message deliberately leaves out. */
private fun logTrashFailures(report: AndroidTrashReport) {
    if (report.failures.isEmpty()) {
        return
    }
    Log.w(
        TRACK_MENU_TAG,
        report.failures.joinToString(separator = "; ") { failure ->
            // An already-gone row has no path to name.
            "track ${failure.trackId} (${failure.uri.ifEmpty { "no file" }}): ${failure.error}"
        },
    )
}

@Composable
private fun TrackDeletionConfirmation(
    target: TrackDeletionTarget?,
    dismiss: () -> Unit,
    report: (String) -> Unit,
) {
    val controls = LocalPlaybackControls.current
    target ?: return
    val title = if (target.trackCount == 1L) {
        "Delete ${target.label}?"
    } else {
        "Delete ${target.trackCount} tracks from ${target.label}?"
    }
    AlertDialog(
        onDismissRequest = dismiss,
        title = { Text(title) },
        text = {
            Text("The selected files will be deleted from this device. This cannot be undone.")
        },
        dismissButton = {
            TextButton(onClick = dismiss) { Text("Cancel") }
        },
        confirmButton = {
            TextButton(
                onClick = {
                    dismiss()
                    val ids = runCatching(target.resolveTrackIds).getOrElse { error ->
                        report("Could not load the tracks: ${error.message ?: "unknown error"}")
                        return@TextButton
                    }
                    controls.deleteTracks(ids) { outcome ->
                        report(
                            outcome.fold(
                                onSuccess = { deletion ->
                                    logTrashFailures(deletion)
                                    trashOutcomeMessage(deletion, ids.size)
                                },
                                onFailure = { error ->
                                    Log.w(TRACK_MENU_TAG, "Could not delete tracks", error)
                                    "Could not delete tracks: " +
                                        (error.message ?: "unknown error")
                                },
                            ),
                        )
                    }
                },
            ) { Text("Delete") }
        },
    )
}
