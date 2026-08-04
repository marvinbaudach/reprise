package de.reprise.spike

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.text.style.TextAlign
import kotlinx.coroutines.delay

/**
 * How long a transient message stays readable before it dismisses itself.
 */
internal const val TRANSIENT_MESSAGE_MS = 4_000L

/**
 * # Which of the surface's three messages belongs here
 *
 * The Android surface shows failures in two shapes, and they are genuinely
 * different things rather than two spellings of one thing:
 *
 * * **State** persists until something supersedes it. `browseError` is the
 *   outcome of the last browse action and stands until the next one replaces
 *   it; `playback.error` is a field of the snapshot the core hands up, replaced
 *   wholesale by the next 500 ms position tick. Neither is *raised* — both are
 *   read out of whatever the surface currently knows, so neither needs a
 *   lifetime of its own.
 * * **An acknowledgement** is raised once, by one tap, and has no state behind
 *   it to be read out of. A rating that failed is the example: the star does
 *   not change, so without a message the tap looks like it worked, and there is
 *   no later event that would clear the message again. It therefore has to
 *   carry its own dismissal — which is what this type is.
 *
 * A message that outlives the next snapshot and then leaves on its own goes
 * through [TransientMessage] and [TransientMessageText]. A condition the
 * surface can re-read at any time does not.
 *
 * [occurrence] is what makes the same text raised twice a *new* event: without
 * it the dismissal timer would still be running on the first one's schedule and
 * the second message would vanish early.
 */
internal data class TransientMessage(val text: String, val occurrence: Long = 0) {
    fun after(previous: TransientMessage?): TransientMessage =
        if (previous == null) this else copy(occurrence = previous.occurrence + 1)
}

/**
 * Renders [message] and dismisses it after [TRANSIENT_MESSAGE_MS] by calling
 * [onDismissed]. Nothing is drawn while [message] is null.
 *
 * The timer keys on the message itself, so a second failure with identical text
 * restarts the countdown rather than riding out the first one's.
 */
@Composable
internal fun TransientMessageText(message: TransientMessage?, onDismissed: () -> Unit) {
    if (message == null) {
        return
    }
    LaunchedEffect(message) {
        delay(TRANSIENT_MESSAGE_MS)
        onDismissed()
    }
    Text(
        text = message.text,
        color = MaterialTheme.colorScheme.error,
        style = MaterialTheme.typography.bodyMedium,
        textAlign = TextAlign.Center,
    )
}
