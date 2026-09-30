package io.github.marvinbaudach.reprise

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay

/**
 * The most ids one canonical track-id query returns.
 *
 * Mirrors `QUEUE_LIMIT` in `crates/reprise-core/src/queries/queue.rs`: both the
 * album and the artist id queries end in `LIMIT QUEUE_LIMIT` and say nothing
 * when they hit it. The FFI does not expose the constant, and a thin adapter
 * should not grow a getter for one number, so it is repeated here.
 */
internal const val TRACK_ID_QUERY_LIMIT = 10_000

/**
 * How long a deletion may run before its line stops promising progress.
 *
 * A document provider that never answers would otherwise leave "Deleting N
 * tracks…" up for good. The line does not turn into a failure — nothing has
 * failed, as far as anyone knows — it only stops looking like it is about to
 * finish.
 */
internal const val DELETION_STILL_RUNNING_MS = 30_000L

internal const val DELETION_STILL_RUNNING_TEXT = "Still deleting…"

/** One deletion that has started and not yet answered. */
internal fun interface DeletionRun {
    /** The deletion answered. Says [text] and ends this run's progress line. */
    fun finish(text: String)
}

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
    /** Something to say about a deletion that never started. */
    fun say(text: String)

    /**
     * A deletion has started and has not answered yet.
     *
     * Each start is its own run: two deletions may overlap, and the first one
     * to answer must not end the other's progress line.
     */
    fun begin(text: String): DeletionRun
}

internal val LocalDeletionMessages = staticCompositionLocalOf<DeletionMessages?> { null }

/** A row's own slot, for callers that sit outside the library screen. */
internal fun TrackContextMenuAnchorState.asDeletionMessages(): DeletionMessages =
    object : DeletionMessages {
        override fun say(text: String) = this@asDeletionMessages.say(text)

        override fun begin(text: String): DeletionRun {
            this@asDeletionMessages.say(text)
            return DeletionRun { outcome -> this@asDeletionMessages.say(outcome) }
        }
    }

/** The running deletion whose line is showing: the latest one started. */
internal data class DeletionProgress(val run: Long, val text: String)

/**
 * The screen's line for [MobileSurfaceViewModel.deletionProgress] and
 * [MobileSurfaceViewModel.deletionMessage].
 *
 * Progress stays until the deletion answers: a timed message would dismiss
 * itself in the middle of a deletion that takes longer than
 * [TRANSIENT_MESSAGE_MS], which a few hundred files on a document provider do.
 * It is bounded all the same, by [DELETION_STILL_RUNNING_MS].
 */
@Composable
internal fun DeletionMessageLine(surface: MobileSurfaceViewModel) {
    val progress = surface.deletionProgress
    val message = surface.deletionMessage
    if (progress == null && message == null) {
        return
    }
    Column(modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 4.dp)) {
        if (progress != null) {
            var stale by remember(progress.run) { mutableStateOf(false) }
            LaunchedEffect(progress.run) {
                delay(DELETION_STILL_RUNNING_MS)
                stale = true
            }
            Text(
                text = if (stale) DELETION_STILL_RUNNING_TEXT else progress.text,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                style = MaterialTheme.typography.bodyMedium,
                textAlign = TextAlign.Center,
                modifier = Modifier.fillMaxWidth(),
            )
        }
        TransientMessageText(message, surface::dismissDeletionMessage)
    }
}

internal fun deletingMessage(count: Int): String =
    "Deleting $count ${if (count == 1) "track" else "tracks"}…"

internal const val SELECTION_TOO_LARGE_TO_DELETE =
    "This selection is too large to delete at once. Delete it in smaller parts."
