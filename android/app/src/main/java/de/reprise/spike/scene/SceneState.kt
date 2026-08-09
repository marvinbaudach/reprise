package de.reprise.spike.scene

data class Transient(
    val bandIndex: Int,
    val excess: Float,
)

/** The deterministic scene state stepped only by consumed spectrogram frames. */
class SceneState(
    private val frames: SpectrogramFrames,
    val coreShape: CoreShape,
) {
    private val fogEnvelopes = BandEnvelopes.fog(frames.bandCount, frames.frameRateHz)
    private val burstEnvelopes = BandEnvelopes.burst(frames.bandCount, frames.frameRateHz)
    private val rawBands = FloatArray(frames.bandCount)
    private val targets = FloatArray(frames.bandCount)
    private var lastFrameIndex: Int? = null

    val fogBands: FloatArray
        get() = fogEnvelopes.values
    val burstBands: FloatArray
        get() = burstEnvelopes.values
    var level: Float = 0f
        private set
    var bass: Float = 0f
        private set
    var transient: Transient? = null
        private set
    var fogAngleA: Float = 0f
        private set
    var fogAngleB: Float = 0f
        private set
    var revision: Int = 0
        private set

    fun advanceTo(frameIndex: Int) {
        if (frames.frameCount == 0) return
        val targetIndex = frames.clampFrameIndex(frameIndex)
        val previous = lastFrameIndex
        if (previous == targetIndex) return
        if (previous == null && targetIndex > SEEK_FRAMES) {
            resetTo(targetIndex)
            return
        }
        if (previous != null && (targetIndex < previous || targetIndex - previous > SEEK_FRAMES)) {
            resetTo(targetIndex)
            return
        }
        val first = previous?.plus(1) ?: 0
        for (index in first..targetIndex) step(index)
        lastFrameIndex = targetIndex
    }

    fun resetTo(frameIndex: Int) {
        if (frames.frameCount == 0) return
        val targetIndex = frames.clampFrameIndex(frameIndex)
        readRaw(targetIndex)
        val fogChanged = fogEnvelopes.adopt(rawBands)
        val burstChanged = burstEnvelopes.adopt(rawBands)
        val oldLevel = level
        val oldBass = bass
        val oldTransient = transient
        level = mean(burstBands)
        bass = bassMean(burstBands)
        transient = strongestTransient(rawBands, fogBands)
        lastFrameIndex = targetIndex
        if (
            fogChanged || burstChanged || level.changedFrom(oldLevel) || bass.changedFrom(oldBass) ||
            transient != oldTransient
        ) {
            revision += 1
        }
    }

    private fun step(frameIndex: Int) {
        readRaw(frameIndex)
        targets.indices.forEach { band ->
            targets[band] = Lookahead.target(frames, frameIndex, band)
        }
        val fogChanged = fogEnvelopes.step(targets)
        val burstChanged = burstEnvelopes.step(targets)
        val oldLevel = level
        val oldBass = bass
        val oldTransient = transient
        val oldAngleA = fogAngleA
        val oldAngleB = fogAngleB
        level = mean(burstBands)
        bass = bassMean(burstBands)
        transient = strongestTransient(rawBands, fogBands)
        fogAngleA = EnergyIntegrator.advance(fogAngleA, level, FOG_FACTOR_A)
        fogAngleB = EnergyIntegrator.advance(fogAngleB, level, FOG_FACTOR_B)
        if (
            fogChanged || burstChanged || level.changedFrom(oldLevel) || bass.changedFrom(oldBass) ||
            transient != oldTransient || fogAngleA.changedFrom(oldAngleA) ||
            fogAngleB.changedFrom(oldAngleB)
        ) {
            revision += 1
        }
    }

    private fun readRaw(frameIndex: Int) {
        rawBands.indices.forEach { band ->
            rawBands[band] = frames.band(frameIndex, band) / 255f
        }
    }

    private fun strongestTransient(raw: FloatArray, fog: FloatArray): Transient? {
        var strongestBand = -1
        var strongestExcess = TRANSIENT_THRESHOLD
        raw.indices.forEach { band ->
            val excess = raw[band] - fog[band]
            if (excess > strongestExcess) {
                strongestBand = band
                strongestExcess = excess
            }
        }
        return if (strongestBand < 0) null else Transient(strongestBand, strongestExcess)
    }

    private fun mean(values: FloatArray): Float =
        if (values.isEmpty()) 0f else values.sum() / values.size

    private fun bassMean(values: FloatArray): Float {
        val count = minOf(BASS_BAND_COUNT, values.size)
        if (count == 0) return 0f
        var sum = 0f
        repeat(count) { sum += values[it] }
        return sum / count
    }

    private fun Float.changedFrom(previous: Float): Boolean = toRawBits() != previous.toRawBits()

    private companion object {
        const val SEEK_FRAMES = 20
        const val BASS_BAND_COUNT = 4
        const val TRANSIENT_THRESHOLD = 0.18f
        const val FOG_FACTOR_A = 0.9f
        const val FOG_FACTOR_B = -0.6f
    }
}
