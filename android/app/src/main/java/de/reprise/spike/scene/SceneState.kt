package de.reprise.spike.scene

/** Signal state stepped by spectrogram frames, with wall-time base drift applied separately. */
class SceneState(
    private val frames: SpectrogramFrames,
) {
    private val fogEnvelopes = BandEnvelopes.fog(frames.bandCount, frames.frameRateHz)
    private val motionEnvelopes = BandEnvelopes.motion(frames.bandCount, frames.frameRateHz)
    private val rawBands = FloatArray(frames.bandCount)
    private val targets = FloatArray(frames.bandCount)
    private val projectedMotion = FloatArray(frames.bandCount)
    private var lastFrameIndex: Int? = null

    /**
     * The live follower array, handed out by reference on purpose.
     *
     * The canvas reads its band arrays repeatedly, so a defensive copy per read
     * would allocate on the frame path this class exists to feed. The contract
     * instead: the scene is stepped and read on the main thread only, and callers
     * treat the array as read-only.
     * Anything that must outlive the next [advanceTo] takes its own `copyOf()`.
     */
    val fogBands: FloatArray
        get() = fogEnvelopes.values

    /** The fast follower bank that keeps fog rotation responsive to the music. */
    val motionBands: FloatArray
        get() = motionEnvelopes.values
    var fogLevel: Float = 0f
        private set
    var motionLevel: Float = 0f
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
        val motionChanged = motionEnvelopes.adopt(rawBands)
        val oldFogLevel = fogLevel
        val oldMotionLevel = motionLevel
        fogLevel = mean(fogBands).coerceIn(0f, 1f)
        motionLevel = mean(motionBands)
        lastFrameIndex = targetIndex
        if (
            fogChanged || motionChanged || fogLevel.changedFrom(oldFogLevel) ||
            motionLevel.changedFrom(oldMotionLevel)
        ) {
            revision += 1
        }
    }

    /**
     * Reads the motion followers [frameFraction] of their way into the step
     * towards the next measured frame, without stepping anything.
     *
     * The analysis runs at 20 Hz against a display asking for three pictures per
     * frame, so a consumer handed [motionBands] alone sees the same numbers for
     * ~33 ms at a time. Both ends here are measured: the followers stand on the
     * frame the playhead is in, the target belongs to the frame after it, and
     * the reading is the follower's own curve between the two. A fraction of 1
     * is exactly what the next [advanceTo] will produce, so the reading never
     * runs ahead of the music.
     *
     * Fog is deliberately left out: its angles integrate whole frames, and
     * reading between them must not lend them a second step. The array is
     * handed out by reference under the same read-only contract as [motionBands]
     * and is overwritten by the next call.
     */
    fun motionBandsWithin(frameFraction: Float): FloatArray {
        val currentIndex = lastFrameIndex ?: return motionBands
        if (frames.frameCount == 0) return motionBands
        val nextIndex = frames.clampFrameIndex(currentIndex + 1)
        targets.indices.forEach { band ->
            targets[band] = Lookahead.target(frames, nextIndex, band)
        }
        motionEnvelopes.projectInto(projectedMotion, targets, frameFraction)
        return projectedMotion
    }

    /**
     * Drives the haze from wall time alone, for tracks the desktop never
     * analysed.
     *
     * Three sine waves whose periods share no common multiple never repeat
     * within a listening session, which is what "random" has to mean here: a
     * new value per frame would be noise, and noise does not drift. The mix
     * moves the fog's density and speeds its rotation up and down, so a track
     * without analysis still looks alive rather than merely rotating.
     */
    fun wanderTo(totalSeconds: Float, elapsedSeconds: Float) {
        val mix = wanderMix(totalSeconds)
        val oldFogLevel = fogLevel
        fogLevel = (WANDER_CENTRE + WANDER_SWING * mix).coerceIn(0f, 1f)
        advanceFogBy(elapsedSeconds * (1f + WANDER_SPEED_SWING * mix))
        if (fogLevel.changedFrom(oldFogLevel)) {
            revision += 1
        }
    }

    /** Keeps both fog layers breathing even when playback has no new signal frame. */
    fun advanceFogBy(elapsedSeconds: Float) {
        if (elapsedSeconds <= 0f) return
        val oldAngleA = fogAngleA
        val oldAngleB = fogAngleB
        fogAngleA = EnergyIntegrator.wrap360(
            fogAngleA + FOG_BASE_DEGREES_PER_SECOND * elapsedSeconds,
        )
        fogAngleB = EnergyIntegrator.wrap360(
            fogAngleB + FOG_BASE_DEGREES_PER_SECOND_B * elapsedSeconds,
        )
        if (fogAngleA.changedFrom(oldAngleA) || fogAngleB.changedFrom(oldAngleB)) {
            revision += 1
        }
    }

    private fun step(frameIndex: Int) {
        readRaw(frameIndex)
        targets.indices.forEach { band ->
            targets[band] = Lookahead.target(frames, frameIndex, band)
        }
        val fogChanged = fogEnvelopes.step(targets)
        val motionChanged = motionEnvelopes.step(targets)
        val oldFogLevel = fogLevel
        val oldMotionLevel = motionLevel
        val oldAngleA = fogAngleA
        val oldAngleB = fogAngleB
        fogLevel = mean(fogBands).coerceIn(0f, 1f)
        motionLevel = mean(motionBands)
        fogAngleA = EnergyIntegrator.advance(fogAngleA, motionLevel, FOG_FACTOR_A)
        fogAngleB = EnergyIntegrator.advance(fogAngleB, motionLevel, FOG_FACTOR_B)
        if (
            fogChanged || motionChanged || fogLevel.changedFrom(oldFogLevel) ||
            motionLevel.changedFrom(oldMotionLevel) ||
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

    private fun mean(values: FloatArray): Float =
        if (values.isEmpty()) 0f else values.sum() / values.size

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
        const val FOG_FACTOR_A = 0.9f
        const val FOG_FACTOR_B = -0.6f
        const val FOG_BASE_DEGREES_PER_SECOND = 360f / (4f * 60f)
        const val FOG_BASE_DEGREES_PER_SECOND_B =
            FOG_BASE_DEGREES_PER_SECOND * FOG_FACTOR_B / FOG_FACTOR_A

        /** The unanalysed wander: centre density, how far it swings, how much it hurries. */
        const val WANDER_CENTRE = 0.42f
        const val WANDER_SWING = 0.30f
        const val WANDER_SPEED_SWING = 0.7f

        private const val TAU = 6.2831855f

        /** −1..1, from three periods with no common multiple. */
        fun wanderMix(totalSeconds: Float): Float {
            val a = kotlin.math.sin(totalSeconds * TAU / 7.3f)
            val b = kotlin.math.sin(totalSeconds * TAU / 11.7f + 1.7f)
            val c = kotlin.math.sin(totalSeconds * TAU / 19.1f + 3.1f)
            return (a * 0.5f + b * 0.33f + c * 0.17f).coerceIn(-1f, 1f)
        }
    }
}
