package io.github.marvinbaudach.reprise

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp

/**
 * Where a deletion says what it is doing and what it did.
 *
 * A deletion removes the row, page or list it was started from, and a message
 * kept on any of those goes with it — a partial deletion ("2 of 12 could not
 * be deleted") would be reported to nobody. [LocalDeletionMessages] therefore
 * points at a sink owned by the screen when there is one, and falls back to the
 * row's own acknowledgement slot when there is not.
 */
internal interface DeletionMessages {
    /** The deletion has started and has not answered yet. */
    fun progress(text: String)

    /** The deletion answered, or never started. Replaces any [progress]. */
    fun result(text: String)
}

internal val LocalDeletionMessages = staticCompositionLocalOf<DeletionMessages?> { null }

/** A row's own slot, for callers that sit outside the library screen. */
internal fun TrackContextMenuAnchorState.asDeletionMessages(): DeletionMessages =
    object : DeletionMessages {
        override fun progress(text: String) = say(text)

        override fun result(text: String) = say(text)
    }

/**
 * The screen's line for [MobileSurfaceViewModel.deletionProgress] and
 * [MobileSurfaceViewModel.deletionMessage].
 *
 * Progress stays until the deletion answers: a timed message would dismiss
 * itself in the middle of a deletion that takes longer than
 * [TRANSIENT_MESSAGE_MS], which a few hundred files on a document provider do.
 */
@Composable
internal fun DeletionMessageLine(surface: MobileSurfaceViewModel) {
    val progress = surface.deletionProgress
    val message = surface.deletionMessage
    if (progress == null && message == null) {
        return
    }
    Box(modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 4.dp)) {
        if (progress != null) {
            Text(
                text = progress,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                style = MaterialTheme.typography.bodyMedium,
                textAlign = TextAlign.Center,
                modifier = Modifier.fillMaxWidth(),
            )
        } else {
            TransientMessageText(message, surface::dismissDeletionMessage)
        }
    }
}

internal fun deletingMessage(count: Int): String =
    "Deleting $count ${if (count == 1) "track" else "tracks"}…"
