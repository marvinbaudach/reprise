package de.reprise.spike.scene

/**
 * The follower that stands between the beat detector and the fog.
 *
 * The fog used to read the kick more or less directly, and on a 180 bpm double
 * bass that is sixteen impulses a second: the layer flickered at the tempo of
 * the drummer's right foot. This is the piece that stops it. A kick pulls the
 * envelope up over [ATTACK_SECONDS] and it falls back over
 * [RELEASE_SECONDS] — five times slower — so a run of kicks lands on a value
 * that never gets the chance to come down, and the fog simply stays lit.
 *
 * What reaches the film afterwards is narrower still. [level] spends
 * [FLOOR] of its range standing still and lets the music move only the
 * remaining fifth, which is why a section change is visible in the fog and a
 * single snare is not.
 *
 * The step is `dt / tau`, clamped, rather than the exponential
 * [BandEnvelopes] uses. It is the frame-rate-independent form of the same
 * curve for the step sizes a display produces, and it is the form the design
 * was tuned against, so it is the form kept here.
 */
class OilFilmEnvelope {
    var value: Float = 0f
        private set

    /** Where the fog actually sits: [FLOOR] plus the fifth the music owns. */
    val level: Float
        get() = FLOOR + (1f - FLOOR) * value

    /** Steps towards [target] by [elapsedSeconds]; true when the value moved. */
    fun advance(target: Float, elapsedSeconds: Float): Boolean {
        if (elapsedSeconds <= 0f || !elapsedSeconds.isFinite()) return false
        val bounded = if (target.isFinite()) target.coerceIn(0f, 1f) else 0f
        val tau = if (bounded > value) ATTACK_SECONDS else RELEASE_SECONDS
        val step = (elapsedSeconds / tau).coerceAtMost(1f)
        val next = value + (bounded - value) * step
        if (next.toRawBits() == value.toRawBits()) return false
        value = next
        return true
    }

    /** Drops the follower onto [target] outright, for a seek or a track change. */
    fun snapTo(target: Float): Boolean {
        val bounded = if (target.isFinite()) target.coerceIn(0f, 1f) else 0f
        if (bounded.toRawBits() == value.toRawBits()) return false
        value = bounded
        return true
    }

    companion object {
        const val ATTACK_SECONDS = 0.4f
        const val RELEASE_SECONDS = 2.0f
        const val FLOOR = 0.8f
    }
}
