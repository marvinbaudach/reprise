package de.reprise.spike.scene

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class SceneStateTest {
    @Test
    fun replay_is_bit_identical_for_single_steps_and_irregular_redraw_jumps() {
        val frames = patternedFrames(frameCount = 24)
        val oneAtATime = SceneState(frames, CoreShape("Northern Lights", "Reprise"))
        val irregular = SceneState(frames, CoreShape("Northern Lights", "Reprise"))

        (0..23).forEach(oneAtATime::advanceTo)
        listOf(0, 3, 4, 8, 12, 15, 19, 23).forEach(irregular::advanceTo)

        assertArrayEquals(oneAtATime.fogBands, irregular.fogBands, 0f)
        assertArrayEquals(oneAtATime.burstBands, irregular.burstBands, 0f)
        assertEquals(oneAtATime.fogAngleA.toRawBits(), irregular.fogAngleA.toRawBits())
        assertEquals(oneAtATime.fogAngleB.toRawBits(), irregular.fogAngleB.toRawBits())
        assertEquals(oneAtATime.level.toRawBits(), irregular.level.toRawBits())
        assertEquals(oneAtATime.bass.toRawBits(), irregular.bass.toRawBits())
        assertEquals(oneAtATime.transient, irregular.transient)
    }

    @Test
    fun pause_stands_completely_still() {
        val state = SceneState(patternedFrames(frameCount = 24), CoreShape("Still", "Reprise"))
        state.advanceTo(10)
        val fog = state.fogBands.copyOf()
        val burst = state.burstBands.copyOf()
        val level = state.level.toRawBits()
        val bass = state.bass.toRawBits()
        val transient = state.transient
        val angleA = state.fogAngleA.toRawBits()
        val angleB = state.fogAngleB.toRawBits()
        val revision = state.revision

        repeat(10) { state.advanceTo(10) }

        assertArrayEquals(fog, state.fogBands, 0f)
        assertArrayEquals(burst, state.burstBands, 0f)
        assertEquals(level, state.level.toRawBits())
        assertEquals(bass, state.bass.toRawBits())
        assertEquals(transient, state.transient)
        assertEquals(angleA, state.fogAngleA.toRawBits())
        assertEquals(angleB, state.fogAngleB.toRawBits())
        assertEquals(revision, state.revision)
    }

    @Test
    fun a_fresh_far_forward_position_adopts_raw_values_without_integrating_missing_music() {
        val state = SceneState(
            constantFrames(cell = 255, frameCount = 30),
            CoreShape("Restored", "Reprise"),
        )

        state.advanceTo(21)

        assertTrue(state.fogBands.all { it == 1f })
        assertTrue(state.burstBands.all { it == 1f })
        assertEquals(1f, state.level, 0f)
        assertEquals(1f, state.bass, 0f)
        assertNull(state.transient)
        assertEquals(0f, state.fogAngleA, 0f)
        assertEquals(0f, state.fogAngleB, 0f)
        assertEquals(1, state.revision)
    }

    @Test
    fun verse_and_breakdown_have_clearly_different_numeric_energy() {
        val verse = constantFrames(cell = 28, frameCount = 30)
        val breakdown = constantFrames(cell = 224, frameCount = 30)
        val verseState = SceneState(verse, CoreShape("Dynamics", "Reprise"))
        val breakdownState = SceneState(breakdown, CoreShape("Dynamics", "Reprise"))

        verseState.advanceTo(19)
        breakdownState.advanceTo(19)

        assertTrue(
            "breakdown ${breakdownState.level} must be at least twice verse ${verseState.level}",
            breakdownState.level >= verseState.level * 2f,
        )
    }

    @Test
    fun empty_analysis_stays_at_rest_and_never_throws() {
        val frames = SpectrogramFrames(bandCount = 24, frameRateHz = 20, cells = byteArrayOf())
        val state = SceneState(frames, CoreShape("No analysis", "Reprise"))

        listOf(0, 1, 20, 80, 2, Int.MAX_VALUE).forEach(state::advanceTo)

        assertEquals(0, frames.frameCount)
        assertEquals(0f, state.level, 0f)
        assertEquals(0f, state.bass, 0f)
        assertNull(state.transient)
        assertTrue(state.fogBands.all { it == 0f })
        assertTrue(state.burstBands.all { it == 0f })
    }

    @Test
    fun spectrogram_positions_and_reads_clamp_at_both_edges() {
        val frames = SpectrogramFrames(
            bandCount = 2,
            frameRateHz = 20,
            cells = byteArrayOf(10, 20, 30, 40),
        )

        assertEquals(2, frames.frameCount)
        assertEquals(0, frames.frameIndexFor(-1))
        assertEquals(0, frames.frameIndexFor(49))
        assertEquals(1, frames.frameIndexFor(50))
        assertEquals(1, frames.frameIndexFor(Long.MAX_VALUE))
        assertEquals(10, frames.band(-20, -10))
        assertEquals(40, frames.band(20, 10))
    }

    private fun patternedFrames(frameCount: Int): SpectrogramFrames {
        val cells = ByteArray(frameCount * 24) { index ->
            val frame = index / 24
            val band = index % 24
            ((frame * 17 + band * 7) % 256).toByte()
        }
        return SpectrogramFrames(bandCount = 24, frameRateHz = 20, cells = cells)
    }

    private fun constantFrames(cell: Int, frameCount: Int): SpectrogramFrames = SpectrogramFrames(
        bandCount = 24,
        frameRateHz = 20,
        cells = ByteArray(frameCount * 24) { cell.toByte() },
    )
}
