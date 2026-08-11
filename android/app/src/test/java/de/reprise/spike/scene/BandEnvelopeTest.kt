package de.reprise.spike.scene

import org.junit.Assert.assertEquals
import org.junit.Test

class BandEnvelopeTest {
    @Test
    fun twenty_hertz_coefficients_match_the_specification() {
        val fogAttack = BandEnvelopes.fog(bandCount = 1, frameRateHz = 20)
        fogAttack.step(floatArrayOf(1f))
        assertEquals(0.221f, fogAttack.values[0], 0.001f)
        val fogDecay = BandEnvelopes.fog(bandCount = 1, frameRateHz = 20)
        fogDecay.adopt(floatArrayOf(1f))
        fogDecay.step(floatArrayOf(0f))
        assertEquals(0.041f, 1f - fogDecay.values[0], 0.001f)

        val motionAttack = BandEnvelopes.motion(bandCount = 1, frameRateHz = 20)
        motionAttack.step(floatArrayOf(1f))
        assertEquals(0.713f, motionAttack.values[0], 0.001f)
        val motionDecay = BandEnvelopes.motion(bandCount = 1, frameRateHz = 20)
        motionDecay.adopt(floatArrayOf(1f))
        motionDecay.step(floatArrayOf(0f))
        assertEquals(0.204f, 1f - motionDecay.values[0], 0.001f)
    }

    @Test
    fun fog_attack_and_decay_reach_their_one_time_constant_levels() {
        val attack = BandEnvelopes.fog(bandCount = 1, frameRateHz = 20)
        repeat(4) { attack.step(floatArrayOf(1f)) }
        assertEquals(0.63f, attack.values[0], 0.02f)

        val decay = BandEnvelopes.fog(bandCount = 1, frameRateHz = 20)
        decay.adopt(floatArrayOf(1f))
        repeat(24) { decay.step(floatArrayOf(0f)) }
        assertEquals(0.37f, decay.values[0], 0.02f)
    }

    @Test
    fun motion_attack_and_decay_reach_their_one_time_constant_levels() {
        val attack = BandEnvelopes(
            bandCount = 1,
            frameMs = 40f,
            attackMs = 40f,
            decayMs = 220f,
        )
        attack.step(floatArrayOf(1f))
        assertEquals(0.63f, attack.values[0], 0.02f)

        val decay = BandEnvelopes(
            bandCount = 1,
            frameMs = 20f,
            attackMs = 40f,
            decayMs = 220f,
        )
        decay.adopt(floatArrayOf(1f))
        repeat(11) { decay.step(floatArrayOf(0f)) }
        assertEquals(0.37f, decay.values[0], 0.02f)
    }

    @Test
    fun lookahead_lifts_a_future_rise_but_never_pulls_a_current_value_down() {
        val risingCells = ByteArray(9) { 0 }.also { it[8] = 255.toByte() }
        val fallingCells = ByteArray(9) { 255.toByte() }.also { it[8] = 0 }
        val rising = SpectrogramFrames(1, 20, risingCells)
        val falling = SpectrogramFrames(1, 20, fallingCells)

        assertEquals(1f, Lookahead.target(rising, frameIndex = 0, band = 0), 0f)
        assertEquals(1f, Lookahead.target(falling, frameIndex = 0, band = 0), 0f)
    }
}
