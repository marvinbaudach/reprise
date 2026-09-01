package io.github.marvinbaudach.reprise

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class NowPlayingCueTest {
    @Test
    fun first_render_and_same_track_updates_never_fire_confirmation_cues() {
        val gate = TrackChangeCueGate()

        assertFalse(gate.observe(trackId = 10, animationsEnabled = true))
        assertFalse(gate.observe(trackId = 10, animationsEnabled = true))
        assertTrue(gate.observe(trackId = 11, animationsEnabled = true))
        assertFalse(gate.observe(trackId = 11, animationsEnabled = true))
    }

    @Test
    fun reduced_motion_suppresses_every_confirmation_cue() {
        val gate = TrackChangeCueGate()
        gate.observe(trackId = 10, animationsEnabled = false)

        assertFalse(gate.observe(trackId = 11, animationsEnabled = false))
    }

    @Test
    fun waveform_build_fires_once_per_real_cue_and_never_replays_after_reduced_motion() {
        val trigger = WaveformBuildTrigger()

        assertFalse(trigger.observe(cueRevision = 0, animationsEnabled = true))
        assertTrue(trigger.observe(cueRevision = 1, animationsEnabled = true))
        assertFalse(trigger.observe(cueRevision = 1, animationsEnabled = true))
        assertFalse(trigger.observe(cueRevision = 2, animationsEnabled = false))
        assertFalse(trigger.observe(cueRevision = 2, animationsEnabled = true))
        assertTrue(trigger.observe(cueRevision = 3, animationsEnabled = true))
    }

    @Test
    fun top_edge_preindicator_references_the_commit_threshold_symbol() {
        val source = File("src/main/java/de/reprise/spike/TopEdgeAccentLine.kt").readText()

        assertTrue("preindicator must read the commit symbol", TRACK_COMMIT_DISTANCE_FRACTION_SYMBOL in source)
        assertFalse("preindicator must not copy the threshold literal", "0.22" in source)
    }

    @Test
    fun top_edge_preindicator_reaches_full_scale_at_the_commit_distance_from_each_edge() {
        val widthPx = 500f
        val thresholdPx = widthPx * TRACK_COMMIT_DISTANCE_FRACTION

        val forward = topEdgeAccentTransform(thresholdPx, widthPx)
        val backward = topEdgeAccentTransform(-thresholdPx, widthPx)

        assertEquals(1f.toRawBits(), forward.scaleX.toRawBits())
        assertEquals(1f, forward.transformOrigin.pivotFractionX, 0f)
        assertEquals(1f.toRawBits(), backward.scaleX.toRawBits())
        assertEquals(0f, backward.transformOrigin.pivotFractionX, 0f)
    }

    private companion object {
        const val TRACK_COMMIT_DISTANCE_FRACTION_SYMBOL = "TRACK_COMMIT_DISTANCE_FRACTION"
    }
}
