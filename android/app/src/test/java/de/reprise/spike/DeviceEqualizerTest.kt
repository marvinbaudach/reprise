package de.reprise.spike

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class DeviceEqualizerTest {
    /**
     * The curve arithmetic itself is the core's, and is tested there
     * (`playback_settings_tests.rs`). What belongs to this class is everything
     * around it: reading the live engine's real bands, handing them over, and
     * turning the answer into the whole millibel levels the device's API takes.
     */
    @Test
    fun liveSessionDefinesEveryBandAndAChangedSessionRebuildsTheEffect() {
        val first = FakeEqualizerEngine(
            centersMilliHz = listOf(100_000, 1_000_000, 10_000_000),
            levelRangeMilliBel = -1_200..1_200,
        )
        val second = FakeEqualizerEngine(
            centersMilliHz = listOf(250_000, 4_000_000),
            levelRangeMilliBel = -600..600,
        )
        val factory = RecordingEqualizerFactory(mutableListOf(first, second))
        val projector = RecordingProjector(
            mutableListOf(
                listOf(-6.0, 0.0, 6.0),
                // Beyond what the second engine can reach, on purpose.
                listOf(3.5, -9.0),
            ),
        )
        val equalizer = DeviceEqualizer(factory, projector)
        val curve = listOf(
            EqualizerCurvePoint(frequencyHz = 100.0, gainDb = -6.0),
            EqualizerCurvePoint(frequencyHz = 10_000.0, gainDb = 6.0),
        )
        equalizer.configure(enabled = true, curve = curve)

        equalizer.onAudioSessionChanged(41)

        assertEquals(listOf(41), factory.sessions)
        // The live engine's own centres and range are what the projection is
        // asked about — not a guess at five or ten fixed bands.
        assertEquals(curve, projector.requestedCurves.single())
        assertEquals(
            listOf(
                DeviceEqualizerBandCapability(100.0, -12.0, 12.0),
                DeviceEqualizerBandCapability(1_000.0, -12.0, 12.0),
                DeviceEqualizerBandCapability(10_000.0, -12.0, 12.0),
            ),
            projector.requestedBands.single(),
        )
        assertEquals(listOf(-600, 0, 600), first.writtenLevelsMilliBel)
        assertTrue(first.enabled)
        assertEquals(listOf(100.0, 1_000.0, 10_000.0), equalizer.snapshot()!!.bands.map { it.frequencyHz })

        equalizer.onAudioSessionChanged(73)

        assertTrue("the old session effect must release its native resources", first.released)
        assertEquals(listOf(41, 73), factory.sessions)
        assertEquals(
            listOf(
                DeviceEqualizerBandCapability(250.0, -6.0, 6.0),
                DeviceEqualizerBandCapability(4_000.0, -6.0, 6.0),
            ),
            projector.requestedBands.last(),
        )
        // -9 dB is more than this engine can render, so what it is told to do
        // and what the snapshot reports are both the level it can reach.
        assertEquals(listOf(350, -600), second.writtenLevelsMilliBel)
        assertEquals(listOf(3.5, -6.0), equalizer.snapshot()!!.bands.map { it.gainDb })
        assertEquals(listOf(250.0, 4_000.0), equalizer.snapshot()!!.bands.map { it.frequencyHz })
    }
}

private class RecordingEqualizerFactory(
    private val engines: MutableList<FakeEqualizerEngine>,
) : EqualizerEngineFactory {
    val sessions = mutableListOf<Int>()

    override fun create(audioSessionId: Int): EqualizerEngine {
        sessions += audioSessionId
        return engines.removeFirst()
    }
}

private class RecordingProjector(
    private val answers: MutableList<List<Double>>,
) : EqualizerCurveProjector {
    val requestedCurves = mutableListOf<List<EqualizerCurvePoint>>()
    val requestedBands = mutableListOf<List<DeviceEqualizerBandCapability>>()

    override fun project(
        curve: List<EqualizerCurvePoint>,
        bands: List<DeviceEqualizerBandCapability>,
    ): List<Double> {
        requestedCurves += curve
        requestedBands += bands
        return answers.removeFirst()
    }
}

private class FakeEqualizerEngine(
    private val centersMilliHz: List<Int>,
    override val levelRangeMilliBel: IntRange,
) : EqualizerEngine {
    override val numberOfBands: Int = centersMilliHz.size
    override var enabled: Boolean = false
    val writtenLevelsMilliBel = mutableListOf<Int>()
    var released = false

    override fun centerFrequencyMilliHz(band: Int): Int = centersMilliHz[band]

    override fun setBandLevelMilliBel(band: Int, level: Int) {
        assertEquals(writtenLevelsMilliBel.size, band)
        writtenLevelsMilliBel += level
    }

    override fun release() {
        released = true
    }
}
