package io.github.marvinbaudach.reprise

import androidx.activity.ComponentActivity
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.test.assertIsDisplayed
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
    fun aSelectionAtTheQueryLimitDeletesNothingAndSaysWhy() {
        val controls = RecordingContextMenuControls()
        showMenuFor(controls, ids = TRACK_ID_QUERY_LIMIT)

        confirmDeletion()

        assertEquals(emptyList<List<Long>>(), controls.deleted)
        compose.onNodeWithText("too large to delete at once", substring = true).assertIsDisplayed()
    }

    @Test
    fun aSelectionJustUnderTheLimitIsDeletedWhole() {
        val controls = RecordingContextMenuControls()
        showMenuFor(controls, ids = TRACK_ID_QUERY_LIMIT - 1)

        confirmDeletion()

        assertEquals(TRACK_ID_QUERY_LIMIT - 1, controls.deleted.single().size)
    }

    @Test
    fun theLimitMirrorsTheCoreQueueLimit() {
        assertEquals(10_000, TRACK_ID_QUERY_LIMIT)
    }

    private fun showMenuFor(controls: RecordingContextMenuControls, ids: Int) {
        val anchor = TrackContextMenuAnchorState()
        val target = LibraryTrackMenuTarget(
            label = "Big Artist",
            trackCount = ids.toLong(),
            resolveTrackIds = { (1..ids).map(Int::toLong) },
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
    }

    private fun confirmDeletion() {
        compose.onNodeWithText("Delete from device…").performClick()
        compose.onNodeWithText("Delete").performClick()
        compose.waitForIdle()
    }
}
