package io.github.marvinbaudach.reprise.scene

import io.github.marvinbaudach.reprise.VisualBassPressure

/** Signal state stepped by spectrogram frames, with wall-time base drift applied separately. */
class SceneState(
    private val frames: SpectrogramFrames,
) {
    private val filmEnvelope = OilFilmEnvelope()
    private var filmSeconds = 0.0
    private val fogEnvelopes = BandEnvelopes.fog(frames.bandCount, frames.frameRateHz)
    private val motionEnvelopes = BandEnvelopes.motion(frames.bandCount, frames.frameRateHz)
    private val rawBands = FloatArray(frames.bandCount)
    private val frameSeconds = 1f / frames.frameRateHz
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
    var bassPressure: Float = 0f
        private set
    var motionLevel: Float = 0f
        private set
    var fogAngleA: Float = 0f
        private set
    var fogAngleB: Float = 0f
        private set
    var shimmerElapsedSeconds: Double = 0.0
        private set

    /**
     * How far the oil film has drifted, in seconds of wall time.
     *
     * Kept as a `Double` and handed out as a `Float` on purpose. The film's
     * orbits are slow enough that a `Float` carries them perfectly well, but a
     * `Float` *accumulator* does not: past about a day of uninterrupted
     * playback its spacing grows wider than one frame's worth of time, and
     * `t += dt` starts rounding to `t`. The clock would not drift then, it
     * would stop, and the film with it.
     */
    val oilFilmSeconds: Float
        get() = filmSeconds.toFloat()

    /** What the film is lit to — its floor, plus the fifth the music is allowed. */
    val oilFilmLevel: Float
        get() = filmEnvelope.level
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
        val oldBassPressure = bassPressure
        val oldMotionLevel = motionLevel
        // A seek or a track change snaps the drive rather than sliding it there.
        // The rate cap in FogDrive exists against repeated brightness swings, and
        // one step on an action the listener took is not a flash sequence; making
        // it slide would only mean the new track's haze arrives a beat late.
        fogLevel = mean(fogBands).coerceIn(0f, 1f)
        bassPressure = bassMean(motionBands)
        motionLevel = mean(motionBands)
        lastFrameIndex = targetIndex
        // A seek is the one moment the film may jump: sliding its envelope over
        // from the old passage would spend two seconds of release explaining a
        // position change nobody heard.
        val filmSnapped = filmEnvelope.snapTo(bassPressure)
        if (
            filmSnapped || fogChanged || motionChanged || fogLevel.changedFrom(oldFogLevel) ||
            bassPressure.changedFrom(oldBassPressure) ||
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
        if (lastFrameIndex == null || frames.frameCount == 0) return motionBands
        projectMotionWithin(frameFraction)
        return projectedMotion
    }

    /** Reads the fast bass followers between measured frames without integrating fog angles. */
    fun readBassPressureAt(frameFraction: Float): Float {
        if (lastFrameIndex == null || frames.frameCount == 0) return bassPressure
        projectMotionWithin(frameFraction)
        val next = bassMean(projectedMotion)
        if (next.changedFrom(bassPressure)) {
            bassPressure = next
            revision += 1
        }
        return bassPressure
    }

    private fun projectMotionWithin(frameFraction: Float) {
        val currentIndex = checkNotNull(lastFrameIndex)
        val nextIndex = frames.clampFrameIndex(currentIndex + 1)
        targets.indices.forEach { band ->
            targets[band] = Lookahead.target(frames, nextIndex, band)
        }
        motionEnvelopes.projectInto(projectedMotion, targets, frameFraction)
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
        val oldBassPressure = bassPressure
        fogLevel = FogDrive.step(
            fogLevel,
            (WANDER_CENTRE + WANDER_SWING * mix).coerceIn(0f, 1f),
            elapsedSeconds,
        )
        bassPressure = 0f
        advanceFogBy(elapsedSeconds * (1f + WANDER_SPEED_SWING * mix))
        if (fogLevel.changedFrom(oldFogLevel) || bassPressure.changedFrom(oldBassPressure)) {
            revision += 1
        }
    }

    /** Applies the detector's real kick and held pressure without a second envelope. */
    internal fun adoptLiveBassPressure(reading: VisualBassPressure, elapsedSeconds: Float) {
        val kick = reading.kick.finiteUnit()
        val pressure = reading.pressure.finiteUnit()
        val energy = maxOf(kick, pressure)
        val oldFogLevel = fogLevel
        val oldBassPressure = bassPressure
        val oldMotionLevel = motionLevel
        val oldAngleA = fogAngleA
        val oldAngleB = fogAngleB
        fogLevel = FogDrive.step(fogLevel, pressure, elapsedSeconds)
        bassPressure = kick
        motionLevel = energy
        if (elapsedSeconds > 0f) {
            fogAngleA = EnergyIntegrator.wrap360(
                fogAngleA +
                    (FOG_BASE_DEGREES_PER_SECOND + energy * FOG_FACTOR_A * LIVE_REFERENCE_HZ) *
                    elapsedSeconds,
            )
            fogAngleB = EnergyIntegrator.wrap360(
                fogAngleB +
                    (FOG_BASE_DEGREES_PER_SECOND_B + energy * FOG_FACTOR_B * LIVE_REFERENCE_HZ) *
                    elapsedSeconds,
            )
        }
        if (
            fogLevel.changedFrom(oldFogLevel) || bassPressure.changedFrom(oldBassPressure) ||
            motionLevel.changedFrom(oldMotionLevel) || fogAngleA.changedFrom(oldAngleA) ||
            fogAngleB.changedFrom(oldAngleB)
        ) {
            revision += 1
        }
    }

    /**
     * Moves the film on by wall time — its drift, and its envelope.
     *
     * Both halves are deliberately driven from the clock rather than from the
     * analysis frames the rest of this class steps on. The drift has to be
     * music-independent to be worth anything, and the envelope's whole job is
     * to be slower than the music, so neither may inherit the 20 Hz cadence of
     * a spectrogram or the gaps in it.
     *
     * It reads [bassPressure] as its target, so callers step it after whatever
     * updated that this tick — otherwise the envelope chases last frame's kick.
     */
    fun advanceOilFilmBy(elapsedSeconds: Float) {
        if (elapsedSeconds <= 0f || !elapsedSeconds.isFinite()) return
        val previousSeconds = filmSeconds
        filmSeconds += elapsedSeconds.toDouble()
        val envelopeMoved = filmEnvelope.advance(bassPressure, elapsedSeconds)
        if (envelopeMoved || filmSeconds != previousSeconds) revision += 1
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

    /** Advances the artwork disc from unscaled wall time, independent of fog energy. */
    fun advanceShimmerBy(elapsedSeconds: Double) {
        if (elapsedSeconds <= 0.0) return
        val previous = shimmerElapsedSeconds
        shimmerElapsedSeconds = (shimmerElapsedSeconds + elapsedSeconds) % SHIMMER_TURN_SECONDS
        if (shimmerElapsedSeconds != previous) revision += 1
    }

    private fun step(frameIndex: Int) {
        readRaw(frameIndex)
        targets.indices.forEach { band ->
            targets[band] = Lookahead.target(frames, frameIndex, band)
        }
        val fogChanged = fogEnvelopes.step(targets)
        val motionChanged = motionEnvelopes.step(targets)
        val oldFogLevel = fogLevel
        val oldBassPressure = bassPressure
        val oldMotionLevel = motionLevel
        val oldAngleA = fogAngleA
        val oldAngleB = fogAngleB
        fogLevel = FogDrive.step(fogLevel, mean(fogBands).coerceIn(0f, 1f), frameSeconds)
        bassPressure = bassMean(motionBands)
        motionLevel = mean(motionBands)
        fogAngleA = EnergyIntegrator.advance(fogAngleA, motionLevel, FOG_FACTOR_A)
        fogAngleB = EnergyIntegrator.advance(fogAngleB, motionLevel, FOG_FACTOR_B)
        if (
            fogChanged || motionChanged || fogLevel.changedFrom(oldFogLevel) ||
            bassPressure.changedFrom(oldBassPressure) ||
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

    private fun bassMean(values: FloatArray): Float {
        val count = values.size.coerceAtMost(BASS_BAND_COUNT)
        if (count == 0) return 0f
        var total = 0f
        repeat(count) { band -> total += values[band] }
        return (total / count).coerceIn(0f, 1f)
    }

    private fun Float.changedFrom(previous: Float): Boolean = toRawBits() != previous.toRawBits()

    private fun Float.finiteUnit(): Float = if (isFinite()) coerceIn(0f, 1f) else 0f

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
        private const val BASS_BAND_COUNT = 7
        const val FOG_FACTOR_A = 0.9f
        const val FOG_FACTOR_B = -0.6f
        private const val LIVE_REFERENCE_HZ = 20f
        const val FOG_BASE_DEGREES_PER_SECOND = 360f / (4f * 60f)
        const val FOG_BASE_DEGREES_PER_SECOND_B =
            FOG_BASE_DEGREES_PER_SECOND * FOG_FACTOR_B / FOG_FACTOR_A
        private const val SHIMMER_TURN_SECONDS = 60.0

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
