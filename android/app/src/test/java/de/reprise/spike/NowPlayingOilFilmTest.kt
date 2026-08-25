package de.reprise.spike

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.math.abs

class NowPlayingOilFilmTest {
    /**
     * The structural half of the anti-flicker claim.
     *
     * The envelope test shows the level barely moves; this shows that even if
     * it moved all the way, it could not move a cloud. Position and rotation
     * are computed from the clock alone, so the same instant at silence and at
     * full level has to produce the identical pose.
     */
    @Test
    fun the_level_cannot_move_a_cloud_or_turn_it() {
        repeat(NowPlayingOilFilmSpec.cloudCount) { index ->
            listOf(0f, 3.7f, 41.2f, 600f).forEach { seconds ->
                val quiet = NowPlayingOilFilmSpec.cloud(index, seconds, level = 0f)
                val loud = NowPlayingOilFilmSpec.cloud(index, seconds, level = 1f)

                assertEquals(quiet.offsetXDp, loud.offsetXDp, 0f)
                assertEquals(quiet.offsetYDp, loud.offsetYDp, 0f)
                assertEquals(quiet.rotationDegrees, loud.rotationDegrees, 0f)
            }
        }
    }

    /** All the music may do: a tenth of the size, and 0.30 of the opacity. */
    @Test
    fun the_level_owns_only_the_scale_and_the_opacity_and_only_a_little() {
        repeat(NowPlayingOilFilmSpec.cloudCount) { index ->
            val quiet = NowPlayingOilFilmSpec.cloud(index, seconds = 12.5f, level = 0f)
            val loud = NowPlayingOilFilmSpec.cloud(index, seconds = 12.5f, level = 1f)

            assertEquals(NowPlayingOilFilmSpec.LEVEL_SCALE_X, loud.scaleX - quiet.scaleX, 1e-5f)
            assertEquals(NowPlayingOilFilmSpec.LEVEL_SCALE_Y, loud.scaleY - quiet.scaleY, 1e-5f)
            assertTrue(
                "size may swing by a tenth at most",
                loud.scaleX - quiet.scaleX <= 0.10f + 1e-5f,
            )
            val opened = loud.alpha - quiet.alpha
            assertTrue("cloud $index must brighten with the level", opened > 0f)
            assertTrue(
                "cloud $index opacity may swing by $opened, no more than the stated 0.30",
                opened <= NowPlayingOilFilmSpec.ALPHA_SWING + 1e-5f,
            )
        }
    }

    /** The film is background light: no cloud is ever drawn opaque. */
    @Test
    fun no_cloud_is_ever_drawn_opaque() {
        repeat(NowPlayingOilFilmSpec.cloudCount) { index ->
            val brightest = NowPlayingOilFilmSpec.cloud(index, seconds = 0f, level = 1f)

            assertTrue("cloud $index alpha ${brightest.alpha}", brightest.alpha > 0f)
            assertTrue(
                "cloud $index must stay a haze, not a surface: ${brightest.alpha}",
                brightest.alpha <= NowPlayingOilFilmSpec.REST_ALPHA +
                    NowPlayingOilFilmSpec.ALPHA_SWING,
            )
        }
    }

    /** Drift stays inside the amplitudes the design set, harmonic included. */
    @Test
    fun the_drift_stays_within_its_stated_amplitudes() {
        repeat(NowPlayingOilFilmSpec.cloudCount) { index ->
            var widestX = 0f
            var widestY = 0f
            var widestSpin = 0f
            forEachSampledSecond { seconds ->
                val cloud = NowPlayingOilFilmSpec.cloud(index, seconds, level = 0.9f)
                widestX = maxOf(widestX, abs(cloud.offsetXDp))
                widestY = maxOf(widestY, abs(cloud.offsetYDp))
                widestSpin = maxOf(widestSpin, abs(cloud.rotationDegrees))
            }

            assertTrue("cloud $index drifts $widestX dp across", widestX <= 95f)
            assertTrue("cloud $index drifts $widestY dp down", widestY <= 80f)
            assertTrue("cloud $index turns $widestSpin degrees", widestSpin <= 45.0001f)
        }
    }

    /**
     * The orbits never come back into step.
     *
     * Asked as a correlation rather than as a search for coincidences, because
     * the claim is about the shape of the motion, not about any one instant.
     * Two clouds that shared a frequency, or sat a whole turn apart, would
     * track one another over ten minutes and show it here as a coefficient near
     * one or minus one. Six clouds whose frequencies rise on unrelated steps
     * and whose phases step by 2.399 read as near-zero against each other, and
     * that is what a film folding into itself looks like numerically.
     */
    @Test
    fun no_two_clouds_ever_drift_in_step() {
        val traces = List(NowPlayingOilFilmSpec.cloudCount) { index -> driftTrace(index) }

        traces.indices.forEach { a ->
            (a + 1 until traces.size).forEach { b ->
                val correlation = abs(correlation(traces[a], traces[b]))
                assertTrue(
                    "clouds $a and $b drift together at $correlation",
                    correlation < 0.5f,
                )
            }
        }
    }

    /**
     * The film never repeats, even though single clouds do.
     *
     * Worth being exact about, because the looser claim is false. Each cloud's
     * two harmonics stand in a ratio of 2.3 and 1.9 — both rational — so one
     * cloud's own path *is* periodic: cloud zero comes back to its opening pose
     * every seventeen minutes or so. What never comes back is the film, because
     * the six periods are set by six unrelated frequencies and their common
     * multiple runs to hours.
     *
     * So the measurement is taken across the clouds rather than within one: at
     * every lag from a minute out to an hour, at least one cloud has to be
     * somewhere else than it was. Cloud zero going quiet at its own period
     * costs nothing as long as another cloud is still moving, and that is
     * exactly the property the phase step of 2.399 buys.
     */
    @Test
    fun the_film_as_a_whole_never_returns_to_a_pose_it_has_held() {
        var quietestLag = Float.MAX_VALUE
        var quietestAt = 0f

        var lag = 60f
        while (lag <= 3_600f) {
            var loudestCloud = 0f
            repeat(NowPlayingOilFilmSpec.cloudCount) { index ->
                var moved = 0f
                repeat(REPEAT_SAMPLES) { sample ->
                    val at = sample * REPEAT_SAMPLE_STEP
                    moved += poseDistance(
                        NowPlayingOilFilmSpec.cloud(index, at, level = 0.8f),
                        NowPlayingOilFilmSpec.cloud(index, at + lag, level = 0.8f),
                    )
                }
                loudestCloud = maxOf(loudestCloud, moved / REPEAT_SAMPLES)
            }
            if (loudestCloud < quietestLag) {
                quietestLag = loudestCloud
                quietestAt = lag
            }
            lag += 10f
        }

        assertTrue(
            "at a lag of ${quietestAt}s the whole film only moved $quietestLag",
            quietestLag > 5f,
        )
    }

    /** Six clouds, and the field reaches past every edge so none can show one. */
    @Test
    fun the_field_overreaches_the_surface_on_every_side() {
        assertEquals(6, NowPlayingOilFilmSpec.cloudCount)
        assertEquals(0.08f, NowPlayingOilFilmSpec.overscan, 0f)
        assertEquals(2f, NowPlayingOilFilmSpec.flow, 0f)

        repeat(NowPlayingOilFilmSpec.cloudCount) { index ->
            val box = NowPlayingOilFilmSpec.box(index)
            assertTrue("cloud $index has width", box.width > 0f)
            assertTrue("cloud $index has height", box.height > 0f)
            assertTrue("cloud $index starts within the field", box.left > -0.5f)
            assertTrue("cloud $index ends within the field", box.left + box.width < 1.5f)
        }
    }

    private fun poseDistance(a: OilFilmCloud, b: OilFilmCloud): Float =
        abs(a.offsetXDp - b.offsetXDp) +
            abs(a.offsetYDp - b.offsetYDp) +
            abs(a.rotationDegrees - b.rotationDegrees)

    private fun driftTrace(index: Int, samples: Int = SAMPLES): FloatArray = FloatArray(samples) {
        NowPlayingOilFilmSpec.cloud(index, it * SAMPLE_STEP, level = 0.8f).offsetXDp
    }

    private fun correlation(first: FloatArray, second: FloatArray): Float {
        val firstMean = first.average().toFloat()
        val secondMean = second.average().toFloat()
        var covariance = 0.0
        var firstSpread = 0.0
        var secondSpread = 0.0
        first.indices.forEach { index ->
            val a = (first[index] - firstMean).toDouble()
            val b = (second[index] - secondMean).toDouble()
            covariance += a * b
            firstSpread += a * a
            secondSpread += b * b
        }
        if (firstSpread == 0.0 || secondSpread == 0.0) return 0f
        return (covariance / kotlin.math.sqrt(firstSpread * secondSpread)).toFloat()
    }

    private fun forEachSampledSecond(body: (Float) -> Unit) {
        var seconds = 0f
        while (seconds <= 3_600f) {
            body(seconds)
            seconds += 0.25f
        }
    }

    private companion object {
        /** Ten minutes at four samples a second: long enough for the slowest orbit. */
        const val SAMPLE_STEP = 0.25f
        const val SAMPLES = (600f / 0.25f).toInt()

        /** Two minutes of start points is plenty to average one lag over. */
        const val REPEAT_SAMPLES = 120
        const val REPEAT_SAMPLE_STEP = 1f
    }
}
