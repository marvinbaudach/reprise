package io.github.marvinbaudach.reprise

import androidx.activity.ComponentActivity
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertSame
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * Targets one specific claim about the panel loop in [NowPlayingScene]: that
 * `key(panel.track.id, panel.index)` (see the `panels.forEach` there) forces
 * a fresh [FrozenSceneBytes] the moment a neighbour becomes the live panel,
 * because its `panel.index` supposedly changes at that instant.
 *
 * It does not, for this transition. [playPanelWindow] hands out an
 * *absolute* queue position as `index` (`currentIndex + rowIndex -
 * currentRow`), so for one physical row advancing to the adjacent track
 * shifts `currentIndex` and `currentRow` by the same amount and the row's
 * own `index` is unchanged — proven at the data level by
 * [NowPlayingPanelsTest.a_known_index_change_replaces_the_centre_without_discarding_its_neighbour].
 * This test proves the same thing at the Compose `remember`/`key` level that
 * actually matters for [FrozenSceneBytes], for exactly that adjacent
 * swipe-settle. A non-adjacent jump (tap-to-play elsewhere, a clipped window
 * at the ends of the queue, a re-anchor) can still shift the
 * `currentIndex - currentRow` relationship and remount the subtree; this
 * test says nothing about those paths.
 *
 * This test rebuilds the `key(...)`/`remember(...)` nesting locally rather
 * than rendering [NowPlayingScene] itself (that would require a live
 * `NativeVisualSceneEngine`, which needs the native library and is not
 * available to this JVM unit test for a non-live panel). It is therefore
 * evidence for the claim above, not a regression guard: a change to the real
 * `key(panel.track.id, panel.index)` in `NowPlayingScene.kt` would not turn
 * this test red.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class NowPlayingPanelFrozenSceneIdentityTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    fun a_neighbour_s_frozen_scene_survives_becoming_the_live_panel() {
        val current = panelTrack(101)
        val neighbour = panelTrack(102)
        var currentIndex by mutableStateOf(0)
        var neighbourFrozen: FrozenSceneBytes? = null

        compose.setContent {
            val currentTrackId = if (currentIndex == 0) current.id else neighbour.id
            val window = playPanelWindow(currentIndex, currentTrackId, listOf(current, neighbour))
            window.panels.forEach { panel ->
                key(panel.track.id, panel.index) {
                    val frozen = remember(panel.track.id) { FrozenSceneBytes() }
                    if (panel.track.id == neighbour.id) neighbourFrozen = frozen
                }
            }
        }
        compose.waitForIdle()

        // The neighbour drew a real, non-empty scene while it was still off to the side —
        // exactly what a track with a stored spectrogram does before it ever becomes live.
        val drawnWhileNeighbour = byteArrayOf(1, 2, 3, 4)
        val frozenWhileNeighbour = requireNotNull(neighbourFrozen)
        frozenWhileNeighbour.latestOrFrozen(drawnWhileNeighbour)

        // The swipe settles: this track is now the current, live panel.
        currentIndex = 1
        compose.waitForIdle()

        assertSame(
            "the neighbour's FrozenSceneBytes instance must survive becoming the live panel " +
                "-- if key(panel.track.id, panel.index) actually changed here, this would be a " +
                "brand-new, empty instance",
            frozenWhileNeighbour,
            neighbourFrozen,
        )
        // A freshly live engine reports an empty scene until its first ingest -- the frozen
        // buffer must still hand back what was drawn a moment ago instead of nothing.
        assertArrayEquals(drawnWhileNeighbour, requireNotNull(neighbourFrozen).latestOrFrozen(ByteArray(0)))
    }

    private fun panelTrack(id: Long) = LibraryTrack(
        id = id,
        uri = "content://track/$id",
        title = "Track $id",
        artist = "Artist",
        album = "Album",
        durationMs = 120_000,
        playCount = 0,
        rating = 0,
    )
}
