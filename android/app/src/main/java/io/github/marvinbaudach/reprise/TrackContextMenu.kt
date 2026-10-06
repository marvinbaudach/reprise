package io.github.marvinbaudach.reprise

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
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
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
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch
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
 * in a slot underneath.
 *
 * A deletion's outcome is not said here when the row sits on the library
 * screen: the row is often gone by the time the deletion answers, so
 * [LocalDeletionMessages] points at a line the screen owns. Only rows outside
 * it fall back to this slot.
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
    val actions: QueueRowActions,
)

/**
 * The ids to delete, once the selection was resolved: a selection that cannot
 * be deleted is refused, saying why, before anything is hidden. Null after
 * saying it. The ids hidden are the ids later deleted.
 */
private fun deletableIds(ids: List<Long>, messages: DeletionMessages): List<Long>? {
    // A full answer may be a cut one: see TRACK_ID_QUERY_LIMIT.
    if (ids.size >= TRACK_ID_QUERY_LIMIT) {
        messages.say(SELECTION_TOO_LARGE_TO_DELETE)
        return null
    }
    return ids
}

/**
 * The queue row's menu.
 *
 * It has no "Move up"/"Move down" pair. A menu that moves a row one slot per
 * tap is a stand-in for a reorder gesture, and the drag handle beside the row
 * is that gesture: it picks the row up, carries it as far as the thumb goes,
 * and puts it down. Leaving both in would offer two answers to one question,
 * and the slower of the two would be the discoverable one.
 */
@Composable
internal fun TrackContextMenu(
    anchor: TrackContextMenuAnchorState,
    target: QueueTrackMenuTarget,
) {
    val controls = LocalPlaybackControls.current
    val pendingDeletions = LocalDeletionMessages.current?.pendingDeletions
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
            text = { Text("Remove from queue") },
            onClick = {
                anchor.expanded = false
                removeFromQueue(target, controls, pendingDeletions)
            },
        )
    }
}

/**
 * Removes the row and, when the screen can show one, offers to put it back.
 *
 * The undo re-reads the queue page by sending the no-op move (a row onto its
 * own position) through the page's own action: that page reloads after every
 * edit, whatever its answer, and is the one place that knows how.
 */
private fun removeFromQueue(
    target: QueueTrackMenuTarget,
    controls: PlaybackControls,
    pendingDeletions: PendingDeletions?,
) {
    val remove = { target.actions.remove(target.position, target.trackId) }
    if (pendingDeletions == null) {
        remove()
        return
    }
    pendingDeletions.removeFromQueueWithUndo(
        position = target.position,
        trackId = target.trackId,
        playback = controls,
        remove = remove,
        refresh = { target.actions.move(0, target.trackId, 0) },
    )
}

@Composable
internal fun TrackContextMenu(
    anchor: TrackContextMenuAnchorState,
    target: LibraryTrackMenuTarget,
) {
    val controls = LocalPlaybackControls.current
    // The screen's line when there is one: this row may be gone when a
    // deletion answers or a lookup is cancelled.
    val screenMessages = LocalDeletionMessages.current
    val pendingDeletions = screenMessages?.pendingDeletions
    val deletionMessages = screenMessages ?: anchor.asDeletionMessages()
    val scope = rememberCoroutineScope()
    // The id query is not instant for a big artist. While one is out, the
    // menu's items are off: a second tap would ask the catalog the same
    // question again and, for a deletion, hide the same tracks twice.
    var resolving by remember { mutableStateOf(false) }

    fun whileResolving(work: suspend () -> Unit) {
        if (resolving) {
            return
        }
        resolving = true
        scope.launch {
            try {
                work()
            } finally {
                resolving = false
            }
        }
    }

    suspend fun resolvedIds(reportFailure: (String) -> Unit = anchor::say): List<Long>? {
        val outcome = try {
            resolveOffMain(target.resolveTrackIds)
        } catch (cancelled: CancellationException) {
            // The row's line leaves with it, so only the screen can explain
            // why its unfinished action disappeared.
            screenMessages?.say(
                "The list changed before the tracks of ${target.label} were found. Nothing was done.",
            )
            throw cancelled
        }
        return outcome
            .onFailure { error -> reportFailure(couldNotLoadTracks(error)) }
            .getOrNull()
    }

    // Tracks already waiting to be deleted are not there to act on. Saying so
    // when that is all there was: an action that does nothing, silently, reads
    // as a tap that was lost.
    fun withoutPending(ids: List<Long>, say: (String) -> Unit): List<Long>? {
        val visible = pendingDeletions?.withoutHidden(ids) ?: ids
        if (visible.isEmpty() && ids.isNotEmpty()) {
            say(EVERYTHING_TAPPED_IS_BEING_DELETED)
            return null
        }
        return visible
    }

    suspend fun actionableIds(): List<Long>? =
        resolvedIds()?.let { ids -> withoutPending(ids, anchor::say) }

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
        modifier = Modifier.testTag("library-track-context-menu"),
        offset = anchor.touchOffset,
    ) {
        DropdownMenuItem(
            text = { Text("Play") },
            enabled = !resolving,
            onClick = {
                anchor.expanded = false
                whileResolving { actionableIds()?.let(target.play) }
            },
        )
        DropdownMenuItem(
            text = { Text("Play next") },
            enabled = !resolving,
            onClick = {
                anchor.expanded = false
                whileResolving {
                    actionableIds()?.let { ids -> controls.queueTracksNext(ids, ::queued) }
                }
            },
        )
        DropdownMenuItem(
            text = { Text("Add to queue") },
            enabled = !resolving,
            onClick = {
                anchor.expanded = false
                whileResolving {
                    actionableIds()?.let { ids -> controls.queueTracksLast(ids, ::queued) }
                }
            },
        )
        HorizontalDivider()
        DropdownMenuItem(
            text = { Text("Delete from device…") },
            enabled = !resolving,
            onClick = {
                anchor.expanded = false
                whileResolving {
                    resolvedIds(deletionMessages::say)?.let { ids ->
                        // The cap is checked on the whole answer, hidden rows
                        // included: a capped answer stays refused.
                        deletableIds(ids, deletionMessages)
                    }?.let { ids ->
                        withoutPending(ids, deletionMessages::say)
                    }?.let { deletable ->
                        pendingDeletions?.begin(deletable, controls)
                            ?: deletionMessages.say(DELETION_UNAVAILABLE)
                    }
                }
            },
        )
    }
}

@Composable
internal fun NowPlayingTrackContextMenu(track: LibraryTrack) {
    val enabled = LocalNowPlayingActionsEnabled.current
    val controls = LocalPlaybackControls.current
    val screenMessages = LocalDeletionMessages.current
    val pendingDeletions = screenMessages?.pendingDeletions
    var expanded by remember { mutableStateOf(false) }
    var message by remember { mutableStateOf<TransientMessage?>(null) }
    // The screen's deletion line sits under this sheet, so what a delete
    // started here has to say goes beside the button, for as long as the sheet
    // is there to show it, and to the screen's line once it is not.
    val sheetMessages = remember(screenMessages) {
        SheetDeletionMessages(screenMessages) { text ->
            message = TransientMessage(text).after(message)
        }
    }
    DisposableEffect(sheetMessages) {
        sheetMessages.open = true
        onDispose { sheetMessages.open = false }
    }
    LaunchedEffect(enabled) {
        if (!enabled) expanded = false
    }
    // The message needs a slot of its own, exactly as in FavouriteHeartButton
    // next door: as a bare sibling it becomes another cell of the actions Row
    // and squeezes the controls sideways.
    Column {
        Box {
            IconButton(
                enabled = enabled,
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
                        // The one id is already in hand: nothing to resolve.
                        pendingDeletions?.begin(listOf(track.id), controls, sheetMessages)
                            ?: run { message = TransientMessage(DELETION_UNAVAILABLE).after(message) }
                    },
                )
            }
        }
        TransientMessageText(message) { message = null }
    }
}

/** Says a delete's progress beside the sheet's button while it is open, else on the screen. */
private class SheetDeletionMessages(
    private val screen: DeletionMessages?,
    private val showHere: (String) -> Unit,
) : DeletionMessages {
    var open = true

    override fun say(text: String) {
        if (open) showHere(text) else screen?.say(text)
    }

    override fun begin(text: String): DeletionRun {
        if (!open) return screen?.begin(text) ?: DeletionRun { }
        showHere(text)
        return DeletionRun { outcome -> say(outcome) }
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
internal fun logTrashFailures(report: AndroidTrashReport) {
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
