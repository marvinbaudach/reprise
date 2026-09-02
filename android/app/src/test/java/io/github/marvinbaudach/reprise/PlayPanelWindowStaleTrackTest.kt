package io.github.marvinbaudach.reprise

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * What the swipe window may do while the track and the index disagree.
 *
 * The index follows the player and moves the moment `next()` is called; the
 * track follows the metadata query answering for it and arrives later. In that
 * gap the pair describes two different songs, and a window written from it puts
 * the outgoing track on the incoming card — the settle animation then carries
 * the old cover into the centre, which is the flash a swipe used to show.
 */
class PlayPanelWindowStaleTrackTest {

    private fun track(id: Long) = LibraryTrack(
        id = id,
        uri = "content://tracks/$id",
        title = "Track $id",
        artist = "Artist $id",
        album = "Album $id",
        durationMs = 180_000,
        playCount = 0,
        rating = 0,
    )

    private fun window(vararg panels: Pair<Int, Long>): PlayPanelWindow {
        val entries = panels.map { (index, id) -> PlayPanel(index, track(id)) }
        return PlayPanelWindow(
            panels = entries,
            firstIndex = entries.minOf(PlayPanel::index),
            lastIndex = entries.maxOf(PlayPanel::index),
        )
    }

    @Test
    fun `a disagreeing pair does not write the window at all`() {
        val settled = window(4 to 40L, 5 to 50L, 6 to 60L)

        val advanced = settled.advancedTo(
            track = track(50L),
            currentIndex = 6,
            trackIsStale = true,
        )

        assertNull("a stale pair must leave the window alone", advanced)
    }

    @Test
    fun `writing a disagreeing pair is what puts the old cover on the new card`() {
        val settled = window(4 to 40L, 5 to 50L, 6 to 60L)

        // The defect the guard exists for: asked directly, the window happily
        // stamps the outgoing track onto the index the swipe is settling on.
        // This is why the caller must wait rather than write.
        val stamped = settled.withCurrentPanel(track = track(50L), currentIndex = 6)

        assertEquals(
            50L,
            stamped.panels.single { it.index == 6 }.track.id,
        )
    }

    @Test
    fun `an agreeing pair writes the window as before`() {
        val settled = window(4 to 40L, 5 to 50L, 6 to 60L)

        val advanced = settled.advancedTo(
            track = track(60L),
            currentIndex = 6,
            trackIsStale = false,
        )

        assertNotNull(advanced)
        assertEquals(60L, advanced!!.panels.single { it.index == 6 }.track.id)
    }

    @Test
    fun `waiting keeps the settled window untouched`() {
        val settled = window(4 to 40L, 5 to 50L, 6 to 60L)
        val before = settled.panels.map { it.index to it.track.id }

        settled.advancedTo(track(50L), currentIndex = 6, trackIsStale = true)

        assertEquals(before, settled.panels.map { it.index to it.track.id })
    }

    @Test
    fun `the pair that agrees keeps only the reachable neighbours`() {
        val settled = window(4 to 40L, 5 to 50L, 6 to 60L)

        val advanced = settled.advancedTo(
            track = track(60L),
            currentIndex = 6,
            trackIsStale = false,
        )

        assertNotNull(advanced)
        assertTrue(
            "a card more than one step away stayed in the window: ${advanced!!.panels}",
            advanced.panels.all { kotlin.math.abs(it.index - 6) <= 1 },
        )
    }

    @Test
    fun `stepping back waits on a disagreeing pair too`() {
        val settled = window(4 to 40L, 5 to 50L, 6 to 60L)

        val advanced = settled.advancedTo(
            track = track(50L),
            currentIndex = 4,
            trackIsStale = true,
        )

        assertNull(advanced)
    }
}
