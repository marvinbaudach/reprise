package io.github.marvinbaudach.reprise

import androidx.activity.ComponentActivity
import androidx.compose.foundation.layout.Box
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.longClick
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.unit.dp
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/** "Remove from queue" can be taken back for as long as the snackbar offers it. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36], qualifiers = "w412dp-h916dp-port")
class QueueRemovalUndoTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private val harness = DeletionHarness()

    @Test
    fun aRemovedQueueRowComesBackWhereItWasWhenUndoIsPressed() {
        val queue = FakeQueueControls(listOf(41, 42, 43))
        showQueue(queue)

        removeRow(42)

        compose.awaitText("Removed from queue")
        compose.onNodeWithText("Undo").assertIsDisplayed()
        assertEquals(listOf(41L, 43L), queue.upcoming)
        assertRowShown(42, shown = false)

        compose.onNodeWithText("Undo").performClick()
        compose.waitUntil(AWAIT_TIMEOUT_MS) { queue.upcoming == listOf(41L, 42L, 43L) }

        assertRowShown(42, shown = true)
    }

    @Test
    fun anUndoAfterTheQueueChangedShapePutsTheRowNext() {
        val queue = FakeQueueControls(listOf(41, 42, 43))
        showQueue(queue)

        removeRow(43)
        compose.awaitText("Removed from queue")
        queue.upcoming.removeAt(0)
        compose.onNodeWithText("Undo").performClick()
        compose.waitUntil(AWAIT_TIMEOUT_MS) { queue.upcoming.size == 2 }

        assertEquals(listOf(43L, 42L), queue.upcoming)
    }

    @Test
    fun theOfferEndsWhenItsWindowPasses() {
        val queue = FakeQueueControls(listOf(41, 42, 43))
        showQueue(queue)

        removeRow(42)
        compose.awaitText("Removed from queue")
        compose.runOnIdle { harness.passTheWindow() }
        compose.waitForIdle()

        compose.onNodeWithText("Undo").assertDoesNotExist()
        assertEquals(listOf(41L, 43L), queue.upcoming)
    }

    private fun removeRow(trackId: Long) {
        compose.onNodeWithTag("queue-track-row-$trackId").performTouchInput { longClick() }
        compose.onNodeWithText("Remove from queue").performClick()
    }

    private fun assertRowShown(trackId: Long, shown: Boolean) {
        compose.waitUntil(AWAIT_TIMEOUT_MS) {
            compose.onAllNodesWithTag("queue-track-row-$trackId").fetchSemanticsNodes()
                .isNotEmpty() == shown
        }
    }

    private fun showQueue(queue: FakeQueueControls) {
        compose.setContent {
            MaterialTheme {
                CompositionLocalProvider(
                    LocalPlaybackControls provides queue,
                    LocalDeletionMessages provides harness.surface,
                ) {
                    Box {
                        NowPlayingQueuePage(
                            PlaybackUiState().libraryPlayback(),
                            harness.surface,
                            SurfaceLayout.STACKED,
                        )
                        UndoSnackbarHost(harness.surface.pendingDeletions) { 0.dp }
                    }
                }
            }
        }
        compose.waitUntil(AWAIT_TIMEOUT_MS) {
            compose.onAllNodesWithTag("queue-track-row-41").fetchSemanticsNodes().isNotEmpty()
        }
    }
}
