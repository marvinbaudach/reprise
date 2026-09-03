package io.github.marvinbaudach.reprise

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * What a swiping panel is allowed to show.
 *
 * The defect these pin down: cover opacity used to be damped by `near`, so a
 * card away from the centre kept its artwork even with the visualizer on. Both
 * halves of the reported symptom — the incoming neighbour's cover, and the
 * outgoing card's cover fading back in — came from that one term.
 */
class NowPlayingPanelOpacityTest {

    private val tolerance = 1e-4f

    @Test
    fun `neighbour hides its cover while the visualizer is on`() {
        val neighbour = nowPlayingPanelOpacity(
            visualizerOpacity = 1f,
            near = 0f,
        )

        assertEquals(0f, neighbour.cover, tolerance)
    }

    @Test
    fun `the outgoing card does not fade its cover back in as it leaves`() {
        val leaving = (0..10).map { step ->
            nowPlayingPanelOpacity(
                visualizerOpacity = 1f,
                near = 1f - step / 10f,
            ).cover
        }

        assertTrue(
            "cover rose while the card slid away: $leaving",
            leaving.all { it <= tolerance },
        )
    }

    @Test
    fun `a halfway neighbour divides the visualizer between bars and plate`() {
        val neighbour = nowPlayingPanelOpacity(
            visualizerOpacity = 1f,
            near = 0.5f,
        )

        assertTrue(neighbour.bars > 0f)
        assertTrue(neighbour.plate > 0f)
        assertEquals(1f, neighbour.bars + neighbour.plate, tolerance)
    }

    @Test
    fun `bars and plate always add up to the visualizer crossfade`() {
        for (visualizerStep in 0..10) {
            for (nearStep in 0..10) {
                val visualizer = visualizerStep / 10f
                val layers = nowPlayingPanelOpacity(
                    visualizerOpacity = visualizer,
                    near = nearStep / 10f,
                )

                assertEquals(
                    "gap at visualizer=$visualizer near=${nearStep / 10f}",
                    visualizer,
                    layers.bars + layers.plate,
                    tolerance,
                )
            }
        }
    }

    @Test
    fun `every layer stays a usable alpha`() {
        for (visualizerStep in 0..10) {
            for (nearStep in 0..10) {
                val layers = nowPlayingPanelOpacity(
                    visualizerOpacity = visualizerStep / 10f,
                    near = nearStep / 10f,
                )

                listOf(layers.cover, layers.bars, layers.plate).forEach { alpha ->
                    assertTrue("alpha out of range: $alpha", alpha in -tolerance..(1f + tolerance))
                }
            }
        }
    }

    @Test
    fun `a swipe with the visualizer off leaves the cover alone`() {
        for (nearStep in 0..10) {
            val layers = nowPlayingPanelOpacity(
                visualizerOpacity = 0f,
                near = nearStep / 10f,
            )

            assertEquals(1f, layers.cover, tolerance)
            assertEquals(0f, layers.bars, tolerance)
            assertEquals(0f, layers.plate, tolerance)
        }
    }

    @Test
    fun `the centre card keeps its spectrum at rest`() {
        val centre = nowPlayingPanelOpacity(
            visualizerOpacity = 1f,
            near = 1f,
        )

        assertEquals(1f, centre.bars, tolerance)
        assertEquals(0f, centre.plate, tolerance)
        assertEquals(0f, centre.cover, tolerance)
    }

    @Test
    fun `out of range inputs are clamped rather than trusted`() {
        val layers = nowPlayingPanelOpacity(
            visualizerOpacity = 1.4f,
            near = -0.3f,
        )

        assertEquals(0f, layers.cover, tolerance)
        assertEquals(0f, layers.bars, tolerance)
        assertEquals(1f, layers.plate, tolerance)
    }

    @Test
    fun `non-finite inputs produce sane opacities`() {
        val layers = nowPlayingPanelOpacity(
            visualizerOpacity = Float.NaN,
            near = Float.NaN,
        )

        assertEquals(1f, layers.cover, tolerance)
        assertEquals(0f, layers.bars, tolerance)
        assertEquals(0f, layers.plate, tolerance)
    }

    @Test
    fun `bars fall faster than linearly as a panel leaves the centre`() {
        val visualizer = 0.8f
        val bars = listOf(1f, 0.75f, 0.5f, 0.25f, 0f).map { near ->
            nowPlayingPanelOpacity(
                visualizerOpacity = visualizer,
                near = near,
            ).bars
        }

        assertTrue(
            "bars did not decrease monotonically: $bars",
            bars.zipWithNext().all { (a, b) -> a > b },
        )
        assertTrue(
            "midpoint bars did not fall faster than linearly: $bars",
            bars[2] < 0.5f * visualizer,
        )
    }
}
