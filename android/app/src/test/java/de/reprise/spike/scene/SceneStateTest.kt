package de.reprise.spike.scene

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

class SceneStateTest {
    @Test
    fun fog_level_is_the_clamped_mean_of_the_slow_fog_envelopes() {
        val cells = ByteArray(22 * 24) { index ->
            if (index % 24 % 2 == 0) 0 else 255.toByte()
        }
        val state = SceneState(
            SpectrogramFrames(bandCount = 24, frameRateHz = 20, cells = cells),
        )

        state.advanceTo(21)

        assertEquals(0.5f.toRawBits(), state.fogLevel.toRawBits())
    }

    @Test
    fun playing_non_empty_spectrogram_keeps_rotating_fog_from_motion_envelopes() {
        val state = SceneState(
            constantFrames(cell = 192, frameCount = 4),
        )

        state.advanceTo(0)
        val firstAngle = state.fogAngleA
        state.advanceTo(1)

        assertTrue(state.motionBands.any { it > 0f })
        assertTrue(state.motionLevel > 0f)
        assertTrue(state.fogAngleA != firstAngle)
    }

    @Test
    fun replay_is_bit_identical_for_single_steps_and_irregular_redraw_jumps() {
        val frames = patternedFrames(frameCount = 24)
        val oneAtATime = SceneState(frames)
        val irregular = SceneState(frames)

        (0..23).forEach(oneAtATime::advanceTo)
        listOf(0, 3, 4, 8, 12, 15, 19, 23).forEach(irregular::advanceTo)

        assertArrayEquals(oneAtATime.fogBands, irregular.fogBands, 0f)
        assertArrayEquals(oneAtATime.motionBands, irregular.motionBands, 0f)
        assertEquals(oneAtATime.fogAngleA.toRawBits(), irregular.fogAngleA.toRawBits())
        assertEquals(oneAtATime.fogAngleB.toRawBits(), irregular.fogAngleB.toRawBits())
        assertEquals(oneAtATime.motionLevel.toRawBits(), irregular.motionLevel.toRawBits())
    }

    @Test
    fun pause_stands_completely_still() {
        val state = SceneState(patternedFrames(frameCount = 24))
        state.advanceTo(10)
        val fog = state.fogBands.copyOf()
        val motion = state.motionBands.copyOf()
        val motionLevel = state.motionLevel.toRawBits()
        val angleA = state.fogAngleA.toRawBits()
        val angleB = state.fogAngleB.toRawBits()
        val revision = state.revision

        repeat(10) { state.advanceTo(10) }

        assertArrayEquals(fog, state.fogBands, 0f)
        assertArrayEquals(motion, state.motionBands, 0f)
        assertEquals(motionLevel, state.motionLevel.toRawBits())
        assertEquals(angleA, state.fogAngleA.toRawBits())
        assertEquals(angleB, state.fogAngleB.toRawBits())
        assertEquals(revision, state.revision)
    }

    @Test
    fun a_fresh_far_forward_position_adopts_raw_values_without_integrating_missing_music() {
        val state = SceneState(constantFrames(cell = 255, frameCount = 30))

        state.advanceTo(21)

        assertTrue(state.fogBands.all { it == 1f })
        assertTrue(state.motionBands.all { it == 1f })
        assertEquals(1f, state.motionLevel, 0f)
        assertEquals(0f, state.fogAngleA, 0f)
        assertEquals(0f, state.fogAngleB, 0f)
        assertEquals(1, state.revision)
    }

    @Test
    fun a_gap_at_the_cap_is_replayed_frame_by_frame_while_a_seek_of_the_same_width_snaps() {
        val frames = patternedFrames(frameCount = SceneState.CATCH_UP_FRAMES + 2)
        val stepped = SceneState(frames)
        val gapped = SceneState(frames)
        val seeked = SceneState(frames)

        (0..SceneState.CATCH_UP_FRAMES).forEach(stepped::advanceTo)
        gapped.advanceTo(0)
        gapped.advanceTo(SceneState.CATCH_UP_FRAMES, afterMissedFrames = true)
        seeked.advanceTo(0)
        val angleAfterOneFrame = seeked.fogAngleA.toRawBits()
        seeked.advanceTo(SceneState.CATCH_UP_FRAMES)

        assertArrayEquals(stepped.fogBands, gapped.fogBands, 0f)
        assertArrayEquals(stepped.motionBands, gapped.motionBands, 0f)
        assertEquals(stepped.fogAngleA.toRawBits(), gapped.fogAngleA.toRawBits())
        assertEquals(stepped.fogAngleB.toRawBits(), gapped.fogAngleB.toRawBits())
        assertEquals(angleAfterOneFrame, seeked.fogAngleA.toRawBits())
        assertTrue(
            "the replayed gap must carry the skipped energy that the seek discards",
            gapped.fogAngleA != seeked.fogAngleA,
        )
    }

    @Test
    fun a_gap_wider_than_the_cap_snaps_to_the_raw_frame() {
        val frames = patternedFrames(frameCount = SceneState.CATCH_UP_FRAMES + 2)
        val state = SceneState(frames)
        val beyondCap = SceneState.CATCH_UP_FRAMES + 1

        state.advanceTo(0)
        val angleAfterOneFrame = state.fogAngleA.toRawBits()
        state.advanceTo(beyondCap, afterMissedFrames = true)

        assertEquals(angleAfterOneFrame, state.fogAngleA.toRawBits())
        assertArrayEquals(rawBandsAt(frames, beyondCap), state.fogBands, 0f)
    }

    @Test
    fun a_backwards_jump_snaps_even_when_frames_were_missed() {
        val frames = patternedFrames(frameCount = 60)
        val state = SceneState(frames)

        (0..40).forEach(state::advanceTo)
        val angle = state.fogAngleA.toRawBits()
        state.advanceTo(4, afterMissedFrames = true)

        assertEquals(angle, state.fogAngleA.toRawBits())
        assertArrayEquals(rawBandsAt(frames, 4), state.fogBands, 0f)
    }

    @Test
    fun verse_and_breakdown_have_clearly_different_numeric_energy() {
        val verse = constantFrames(cell = 28, frameCount = 30)
        val breakdown = constantFrames(cell = 224, frameCount = 30)
        val verseState = SceneState(verse)
        val breakdownState = SceneState(breakdown)

        verseState.advanceTo(19)
        breakdownState.advanceTo(19)

        assertTrue(
            "breakdown ${breakdownState.motionLevel} must be at least twice verse " +
                verseState.motionLevel,
            breakdownState.motionLevel >= verseState.motionLevel * 2f,
        )
    }

    @Test
    fun empty_analysis_stays_at_rest_and_never_throws() {
        val frames = SpectrogramFrames(bandCount = 24, frameRateHz = 20, cells = byteArrayOf())
        val state = SceneState(frames)

        listOf(0, 1, 20, 80, 2, Int.MAX_VALUE).forEach(state::advanceTo)

        assertEquals(0, frames.frameCount)
        assertEquals(0f, state.motionLevel, 0f)
        assertTrue(state.fogBands.all { it == 0f })
        assertTrue(state.motionBands.all { it == 0f })
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

    @Test
    fun truncated_analysis_is_refused_instead_of_dropping_the_partial_frame() {
        val truncated = assertThrows(IllegalArgumentException::class.java) {
            SpectrogramFrames(bandCount = 24, frameRateHz = 20, cells = ByteArray(24 * 3 + 7))
        }

        assertTrue(truncated.message, truncated.message?.contains("whole frames") == true)
    }

    private fun rawBandsAt(frames: SpectrogramFrames, frameIndex: Int): FloatArray =
        FloatArray(frames.bandCount) { band -> frames.band(frameIndex, band) / 255f }

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
