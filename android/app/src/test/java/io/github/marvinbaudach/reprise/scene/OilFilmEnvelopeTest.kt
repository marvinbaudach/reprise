package io.github.marvinbaudach.reprise.scene

import io.github.marvinbaudach.reprise.NowPlayingOilFilmSpec
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class OilFilmEnvelopeTest {
    /**
     * The measurement the whole change exists for.
     *
     * A 180 bpm double bass is sixteen kicks a second. Driven with that for
     * five seconds, the old coupling would have taken the fog up and down
     * eighty times. What is asserted here is that the film's brightness — not
     * the follower, the brightness the clouds are actually drawn at — moves
     * across a band narrow enough that no single kick is visible in it, and
     * that it never once falls back to where it started.
     */
    @Test
    fun a_180_bpm_double_bass_leaves_the_film_standing_still() {
        val envelope = OilFilmEnvelope()
        val trace = mutableListOf<Float>()
        val step = 1f / FRAME_RATE

        forEachDoubleBassFrame(seconds = 5f, step = step) { frame, kick ->
            envelope.advance(kick, step)
            // Skip the first second: that is the film arriving, not flickering.
            if (frame >= FRAME_RATE) trace += envelope.level
        }

        val lowest = trace.min()
        val highest = trace.max()
        assertTrue(
            "the film must stay lit through the kicks, not fall back to $lowest",
            lowest > OilFilmEnvelope.FLOOR + 0.05f,
        )
        // The number that matters is not the follower's ripple but the opacity
        // the clouds are drawn at, which is the ripple times the swing the
        // level is allowed. Measured here at 0.009 on an opacity near 0.61 —
        // about a sixty-fifth, well under what an eye reads as a flicker.
        val opacityRipple = (highest - lowest) * NowPlayingOilFilmSpec.ALPHA_SWING
        val litOpacity = NowPlayingOilFilmSpec.REST_ALPHA +
            NowPlayingOilFilmSpec.ALPHA_SWING * lowest
        assertTrue(
            "per-kick opacity swing $opacityRipple of $litOpacity must stay under a fiftieth",
            opacityRipple / litOpacity < 0.02f,
        )
    }

    /** The same drum at half the tempo must not start a flicker either. */
    @Test
    fun a_90_bpm_kick_still_never_returns_to_the_floor_between_beats() {
        val envelope = OilFilmEnvelope()
        val step = 1f / FRAME_RATE
        var lowestAfterSettling = 1f

        forEachKickTrain(seconds = 6f, step = step, kicksPerSecond = 1.5f) { frame, kick ->
            envelope.advance(kick, step)
            if (frame >= 2 * FRAME_RATE) {
                lowestAfterSettling = minOf(lowestAfterSettling, envelope.level)
            }
        }

        assertTrue(
            "a beat every 667 ms must not let the film reach its floor: $lowestAfterSettling",
            lowestAfterSettling > OilFilmEnvelope.FLOOR + 0.02f,
        )
    }

    /** Silence is the one thing that does take the film all the way down. */
    @Test
    fun the_film_returns_to_its_floor_over_seconds_of_silence_not_frames() {
        val envelope = OilFilmEnvelope()
        val step = 1f / FRAME_RATE
        repeat(FRAME_RATE) { envelope.advance(1f, step) }

        val lit = envelope.level
        repeat(FRAME_RATE / 2) { envelope.advance(0f, step) }
        val afterHalfASecond = envelope.level
        repeat(FRAME_RATE * 10) { envelope.advance(0f, step) }
        val afterTenSeconds = envelope.level

        assertTrue("half a second must barely move it", afterHalfASecond > lit - 0.06f)
        assertEquals(OilFilmEnvelope.FLOOR, afterTenSeconds, 0.005f)
    }

    /** Rising is five times quicker than falling, which is what holds a run of kicks up. */
    @Test
    fun the_follower_rises_five_times_faster_than_it_falls() {
        assertEquals(
            5f,
            OilFilmEnvelope.RELEASE_SECONDS / OilFilmEnvelope.ATTACK_SECONDS,
            0.001f,
        )

        val rising = OilFilmEnvelope().also { it.advance(1f, 0.1f) }.value
        val falling = OilFilmEnvelope().also {
            it.snapTo(1f)
            it.advance(0f, 0.1f)
        }.value

        assertEquals(0.1f / OilFilmEnvelope.ATTACK_SECONDS, rising, 0.001f)
        assertEquals(1f - 0.1f / OilFilmEnvelope.RELEASE_SECONDS, falling, 0.001f)
    }

    /** Only the top fifth of the film's brightness belongs to the music. */
    @Test
    fun the_music_owns_a_fifth_of_the_range_and_the_floor_owns_the_rest() {
        val envelope = OilFilmEnvelope()

        assertEquals(0.8f, OilFilmEnvelope.FLOOR, 0f)
        assertEquals(OilFilmEnvelope.FLOOR, envelope.level, 0f)
        envelope.snapTo(1f)
        assertEquals(1f, envelope.level, 0f)
    }

    /** A frame longer than the constant lands on the target rather than past it. */
    @Test
    fun an_oversized_frame_clamps_instead_of_overshooting() {
        val envelope = OilFilmEnvelope()

        envelope.advance(1f, elapsedSeconds = 30f)

        assertEquals(1f, envelope.value, 0f)
    }

    /** A stalled or nonsense clock leaves the follower exactly where it was. */
    @Test
    fun a_non_advancing_or_non_finite_step_moves_nothing() {
        val envelope = OilFilmEnvelope().also { it.snapTo(0.5f) }

        assertTrue(!envelope.advance(1f, 0f))
        assertTrue(!envelope.advance(1f, -1f))
        assertTrue(!envelope.advance(1f, Float.NaN))
        assertEquals(0.5f, envelope.value, 0f)
    }

    /** A NaN reading from the detector is read as silence, never propagated. */
    @Test
    fun a_non_finite_target_is_treated_as_silence() {
        val envelope = OilFilmEnvelope().also { it.snapTo(1f) }

        envelope.advance(Float.NaN, 1f)

        assertTrue("value must stay finite", envelope.value.isFinite())
        assertTrue("NaN must pull down, not up", envelope.value < 1f)
    }

    private fun forEachDoubleBassFrame(seconds: Float, step: Float, body: (Int, Float) -> Unit) =
        forEachKickTrain(seconds, step, kicksPerSecond = 16f, body = body)

    /**
     * Feeds a kick train as the detector reports one: a spike at the impulse
     * that decays over ~90 ms, which is the shape the fog used to answer.
     */
    private fun forEachKickTrain(
        seconds: Float,
        step: Float,
        kicksPerSecond: Float,
        body: (Int, Float) -> Unit,
    ) {
        val interval = 1f / kicksPerSecond
        var nextKick = 0f
        var raw = 0f
        val frames = (seconds / step).toInt()
        repeat(frames) { frame ->
            val time = frame * step
            raw = (raw - step / KICK_DECAY_SECONDS).coerceAtLeast(0f)
            if (time >= nextKick) {
                raw = 1f
                nextKick += interval
            }
            body(frame, raw)
        }
    }

    private companion object {
        const val FRAME_RATE = 60
        const val KICK_DECAY_SECONDS = 0.09f
    }
}
