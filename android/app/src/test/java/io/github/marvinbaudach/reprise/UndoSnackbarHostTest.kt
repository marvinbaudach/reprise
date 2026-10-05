package io.github.marvinbaudach.reprise

import android.content.Context
import android.view.accessibility.AccessibilityManager
import androidx.activity.ComponentActivity
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.LifecycleRegistry
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.test.core.app.ApplicationProvider
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

/** The snackbar host: what a tap on Undo does, how long it stays, when it may delete. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36], qualifiers = "w412dp-h916dp-port")
class UndoSnackbarHostTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private val harness = DeletionHarness()
    private val deletions get() = harness.surface.pendingDeletions

    @Test
    fun anUndoTapIsDecidedInTheTapEvenIfTheWindowEndsInTheSameFrame() {
        val queue = FakeQueueControls(listOf(10, 11, 12))
        showHost(queue)
        compose.runOnIdle { deletions.begin(listOf(11), queue) }
        compose.awaitText("1 track will be deleted")

        compose.mainClock.autoAdvance = false
        compose.onNodeWithText("Undo").performClick()
        // The timer wins the race for the next frame.
        compose.runOnIdle { harness.passTheWindow() }
        compose.mainClock.autoAdvance = true
        compose.waitForIdle()

        assertEquals(emptyList<List<Long>>(), queue.deleted)
        assertEquals(listOf(10L, 11L, 12L), queue.upcoming)
    }

    @Test
    fun theOfferIsWordedInTheFutureBecauseNothingIsDeletedYet() {
        val queue = FakeQueueControls(listOf(10, 11, 12))
        showHost(queue)

        compose.runOnIdle { deletions.begin(listOf(10, 11), queue) }

        compose.awaitText("2 tracks will be deleted")
        compose.onNodeWithText("Undo").assertIsDisplayed()
    }

    @Test
    fun theWindowFollowsTheListenersAccessibilityTimeout() {
        val manager = ApplicationProvider.getApplicationContext<Context>()
            .getSystemService(AccessibilityManager::class.java)
        assertEquals(UNDO_WINDOW_MS, undoWindowFor(manager))

        shadowOf(manager).setInteractiveUiTimeout(20_000)

        assertEquals(20_000L, undoWindowFor(manager))
        assertEquals(UNDO_WINDOW_MS, undoWindowFor(null))
    }

    @Test
    fun aWindowThatPassesInTheBackgroundDeletesOnlyWhenItIsOnScreenAgain() {
        val queue = FakeQueueControls(listOf(10, 11, 12))
        val owner = showHost(queue)
        compose.runOnIdle { deletions.begin(listOf(11), queue) }
        compose.awaitText("1 track will be deleted")

        compose.runOnIdle { owner.registry.currentState = Lifecycle.State.CREATED }
        compose.runOnIdle { harness.passTheWindow() }
        compose.waitForIdle()
        assertEquals(emptyList<List<Long>>(), queue.deleted)

        compose.runOnIdle { owner.registry.currentState = Lifecycle.State.RESUMED }
        compose.waitForIdle()

        assertEquals(listOf(listOf(11L)), queue.deleted)
    }

    private fun showHost(queue: FakeQueueControls): TestOwner {
        val owner = TestOwner()
        compose.setContent {
            MaterialTheme {
                CompositionLocalProvider(
                    LocalPlaybackControls provides queue,
                    LocalLifecycleOwner provides owner,
                ) {
                    Box(Modifier.fillMaxSize()) {
                        UndoSnackbarHost(deletions) { 0.dp }
                    }
                }
            }
        }
        return owner
    }
}

private class TestOwner : LifecycleOwner {
    val registry = LifecycleRegistry(this).also { it.currentState = Lifecycle.State.RESUMED }

    override val lifecycle: Lifecycle get() = registry
}
