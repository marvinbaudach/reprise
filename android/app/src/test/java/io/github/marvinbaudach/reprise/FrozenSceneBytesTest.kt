package io.github.marvinbaudach.reprise

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class FrozenSceneBytesTest {
    @Test
    fun a_fresh_instance_has_not_captured_a_scene() {
        assertFalse(FrozenSceneBytes().hasCapturedScene)
    }

    @Test
    fun an_empty_scene_never_counts_as_captured() {
        val frozen = FrozenSceneBytes()

        frozen.latestOrFrozen(ByteArray(0))

        assertFalse(
            "an empty scene must never flip hasCapturedScene — it is not real data",
            frozen.hasCapturedScene,
        )
    }

    @Test
    fun a_non_empty_scene_stays_captured_even_through_a_later_gap() {
        val frozen = FrozenSceneBytes()

        frozen.latestOrFrozen(byteArrayOf(1, 2, 3))
        assertTrue(frozen.hasCapturedScene)

        frozen.latestOrFrozen(ByteArray(0))
        assertTrue("the latch must not reset just because the current tick is empty", frozen.hasCapturedScene)
    }


    @Test
    fun an_empty_scene_falls_back_to_the_last_one_that_was_not() {
        val frozen = FrozenSceneBytes()
        val ingested = byteArrayOf(1, 2, 3, 4)

        // The engine draws its own scene while it has one...
        assertArrayEquals(ingested, frozen.latestOrFrozen(ingested))

        // ...then its engine is swapped (e.g. the panel just became the live one) and the fresh
        // engine has not ingested anything yet — this is the gap the user saw as a blank panel.
        assertArrayEquals(ingested, frozen.latestOrFrozen(ByteArray(0)))
    }

    @Test
    fun a_fresh_non_empty_scene_replaces_the_frozen_one() {
        val frozen = FrozenSceneBytes()
        frozen.latestOrFrozen(byteArrayOf(1, 2, 3))

        val updated = byteArrayOf(9, 9)
        assertArrayEquals(updated, frozen.latestOrFrozen(updated))
    }

    @Test
    fun with_nothing_ever_ingested_the_gap_still_draws_nothing() {
        val frozen = FrozenSceneBytes()

        assertArrayEquals(ByteArray(0), frozen.latestOrFrozen(ByteArray(0)))
    }
}
