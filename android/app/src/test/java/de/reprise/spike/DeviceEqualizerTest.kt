package de.reprise.spike

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class DeviceEqualizerTest {
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
        val equalizer = DeviceEqualizer(factory)
        equalizer.configure(
            enabled = true,
            curve = listOf(
                EqualizerCurvePoint(frequencyHz = 100.0, gainDb = -6.0),
                EqualizerCurvePoint(frequencyHz = 10_000.0, gainDb = 6.0),
            ),
        )

        equalizer.onAudioSessionChanged(41)

        assertEquals(listOf(41), factory.sessions)
        assertEquals(listOf(-600, 0, 600), first.writtenLevelsMilliBel)
        assertTrue(first.enabled)
        assertEquals(listOf(100.0, 1_000.0, 10_000.0), equalizer.snapshot()!!.bands.map { it.frequencyHz })

        equalizer.onAudioSessionChanged(73)

        assertTrue("the old session effect must release its native resources", first.released)
        assertEquals(listOf(41, 73), factory.sessions)
        assertEquals(2, second.writtenLevelsMilliBel.size)
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
