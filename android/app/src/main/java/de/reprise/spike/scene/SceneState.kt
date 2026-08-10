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

    /**
     * The live follower array, handed out by reference on purpose.
     *
     * The burst reads its band arrays once per corona stroke — 168 times per
     * scene pass, twice that once the bloom pass is counted — so a defensive
     * copy per read would allocate some 340 arrays per frame on the very path
     * this class exists to feed. The contract instead: the scene is stepped and
     * read on the main thread only, and callers treat the array as read-only.
     * Anything that must outlive the next [advanceTo] takes its own `copyOf()`.
     */
    val fogBands: FloatArray
        get() = fogEnvelopes.values

    /** The live burst follower array. See [fogBands] for the read-only contract. */
    val burstBands: FloatArray
        get() = burstEnvelopes.values
    var fogLevel: Float = 0f
        private set
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

    /**
     * Steps every frame between the last processed one and [frameIndex].
     *
     * A forward jump wider than [SEEK_FRAMES] is a seek and snaps — unless the
     * caller reports with [afterMissedFrames] that it was kept from ticking
     * across those frames (screen off, app backgrounded) while playback ran on.
     * That gap is continued playback, not a seek, so it is replayed in order up
     * to [CATCH_UP_FRAMES] and only snaps beyond it. A backwards jump is always
     * a seek, gap or no gap.
     */
    fun advanceTo(frameIndex: Int, afterMissedFrames: Boolean = false) {
        if (frames.frameCount == 0) return
        val targetIndex = frames.clampFrameIndex(frameIndex)
        val previous = lastFrameIndex
        if (previous == targetIndex) return
        if (previous == null && targetIndex > SEEK_FRAMES) {
            resetTo(targetIndex)
            return
        }
        val forwardLimit = if (afterMissedFrames) CATCH_UP_FRAMES else SEEK_FRAMES
        if (previous != null && (targetIndex < previous || targetIndex - previous > forwardLimit)) {
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
        val oldFogLevel = fogLevel
        val oldLevel = level
        val oldBass = bass
        val oldTransient = transient
        fogLevel = mean(fogBands).coerceIn(0f, 1f)
        level = mean(burstBands)
        bass = bassMean(burstBands)
        transient = strongestTransient(rawBands, fogBands)
        lastFrameIndex = targetIndex
        if (
            fogChanged || burstChanged || fogLevel.changedFrom(oldFogLevel) ||
            level.changedFrom(oldLevel) || bass.changedFrom(oldBass) || transient != oldTransient
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
        val oldFogLevel = fogLevel
        val oldLevel = level
        val oldBass = bass
        val oldTransient = transient
        val oldAngleA = fogAngleA
        val oldAngleB = fogAngleB
        fogLevel = mean(fogBands).coerceIn(0f, 1f)
        level = mean(burstBands)
        bass = bassMean(burstBands)
        transient = strongestTransient(rawBands, fogBands)
        fogAngleA = EnergyIntegrator.advance(fogAngleA, level, FOG_FACTOR_A)
        fogAngleB = EnergyIntegrator.advance(fogAngleB, level, FOG_FACTOR_B)
        if (
            fogChanged || burstChanged || fogLevel.changedFrom(oldFogLevel) ||
            level.changedFrom(oldLevel) || bass.changedFrom(oldBass) || transient != oldTransient ||
            fogAngleA.changedFrom(oldAngleA) || fogAngleB.changedFrom(oldAngleB)
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

    internal companion object {
        /**
         * How much of a gap a resume may replay: one minute of analysis at the
         * 20 Hz frame rate.
         *
         * One step is ~200 float operations over 24 bands, so the whole minute
         * costs well under a millisecond — a fraction of the frame that draws
         * it — while the glance at a notification, the pocketed phone and the
         * short lock all stay inside it and keep their deterministic trace. A
         * lock long enough to exceed a minute of music has nothing worth
         * replaying, and snapping there keeps the resume from ever visibly
         * paying for it.
         */
        const val CATCH_UP_FRAMES = 1_200
        const val SEEK_FRAMES = 20
        const val BASS_BAND_COUNT = 4
        const val TRANSIENT_THRESHOLD = 0.18f
        const val FOG_FACTOR_A = 0.9f
        const val FOG_FACTOR_B = -0.6f
    }
}
