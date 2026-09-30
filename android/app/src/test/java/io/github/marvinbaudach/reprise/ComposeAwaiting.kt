package io.github.marvinbaudach.reprise

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.ComposeTestRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithText

internal const val AWAIT_TIMEOUT_MS = 5_000L

/**
 * Waits until a node showing [text] is on screen, then asserts it is.
 *
 * The ids behind a menu action are read off the main thread, and Compose's
 * idling does not track that thread: `performClick()` returns before the dialog
 * or the acknowledgement the click leads to has been drawn.
 */
internal fun ComposeTestRule.awaitText(text: String, substring: Boolean = false) {
    waitUntil(AWAIT_TIMEOUT_MS) {
        onAllNodesWithText(text, substring = substring).fetchSemanticsNodes().isNotEmpty()
    }
    onNodeWithText(text, substring = substring).assertIsDisplayed()
}
