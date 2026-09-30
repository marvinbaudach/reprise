package io.github.marvinbaudach.reprise

import androidx.activity.ComponentActivity
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * The id queries behind "delete this album/artist" stop at [TRACK_ID_QUERY_LIMIT]
 * rows, silently. A selection that hits the stop may have been cut short, so
 * deleting what came back would delete part of a selection the dialog named
 * whole.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36], qualifiers = "w412dp-h916dp-port")
class DeletionCapTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    fun aSelectionAtTheQueryLimitIsRefusedBeforeAnyDialogAsksAboutIt() {
        val controls = RecordingContextMenuControls()
        showMenuFor(controls, ids = TRACK_ID_QUERY_LIMIT)

        compose.onNodeWithText("Delete from device…").performClick()
        compose.awaitText("too large to delete at once", substring = true)
        compose.onNodeWithText("Delete 10000 tracks from Big Artist?").assertDoesNotExist()
        compose.onNodeWithText("Delete").assertDoesNotExist()
        assertEquals(emptyList<List<Long>>(), controls.deleted)
    }

    @Test
    fun aSelectionJustUnderTheLimitIsAskedAboutOnceAndDeletedWhole() {
        val controls = RecordingContextMenuControls()
        val resolves = showMenuFor(controls, ids = TRACK_ID_QUERY_LIMIT - 1)

        compose.onNodeWithText("Delete from device…").performClick()
        compose.awaitText("Delete 9999 tracks from Big Artist?")
        compose.onNodeWithText("Delete").performClick()
        compose.waitForIdle()

        assertEquals(TRACK_ID_QUERY_LIMIT - 1, controls.deleted.single().size)
        assertEquals("the ids asked about are the ids deleted", 1, resolves.get())
    }

    @Test
    fun theLimitEqualsTheCoreQueueLimit() {
        assertEquals(rustConstant("crates/reprise-core/src/queries/queue.rs", "QUEUE_LIMIT"), TRACK_ID_QUERY_LIMIT.toLong())
    }

    @Test
    fun theReloadChunkEqualsTheCoreWindowCap() {
        assertEquals(rustConstant("crates/reprise-core/src/queries/mod.rs", "MAX_WINDOW_LIMIT"), RELOAD_CHUNK_LIMIT)
    }

    /** The value of `pub const NAME: … = N;` in a Rust file of this repository. */
    private fun rustConstant(repoRelativePath: String, name: String): Long {
        val start = generateSequence(java.io.File(System.getProperty("user.dir")).absoluteFile) { it.parentFile }
        val file = start.map { java.io.File(it, repoRelativePath) }.firstOrNull { it.isFile }
            ?: error("$repoRelativePath not found above ${System.getProperty("user.dir")}")
        val declaration = Regex("""const $name\s*:[^=]*=\s*([0-9_]+)\s*;""").find(file.readText())
            ?: error("no `const $name` in $repoRelativePath")
        return declaration.groupValues[1].replace("_", "").toLong()
    }

    /** Returns how many times the selection was resolved. */
    private fun showMenuFor(controls: RecordingContextMenuControls, ids: Int): java.util.concurrent.atomic.AtomicInteger {
        val resolves = java.util.concurrent.atomic.AtomicInteger()
        val anchor = TrackContextMenuAnchorState()
        val target = LibraryTrackMenuTarget(
            label = "Big Artist",
            trackCount = ids.toLong(),
            resolveTrackIds = {
                resolves.incrementAndGet()
                (1..ids).map(Int::toLong)
            },
            play = {},
        )
        compose.setContent {
            MaterialTheme {
                CompositionLocalProvider(LocalPlaybackControls provides controls) {
                    Column {
                        TrackContextMenu(anchor = anchor, target = target)
                        TrackContextMenuMessage(anchor)
                    }
                }
            }
        }
        compose.runOnIdle { anchor.expanded = true }
        return resolves
    }
}
