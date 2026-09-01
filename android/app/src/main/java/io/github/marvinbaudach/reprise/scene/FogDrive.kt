package io.github.marvinbaudach.reprise.scene

/**
 * Bounds how fast the haze behind the cover may change brightness.
 *
 * The fog is a full-screen layer, and it used to answer the bass detector's
 * kick directly: every kick lifted both fog layers and the shimmer disc. On
 * metal that is four or more brightness swings a second across most of the
 * display — a photosensitivity hazard rather than a matter of taste. WCAG
 * 2.3.1 draws its line at three general flashes per second.
 *
 * A smaller coefficient would not have fixed it, because the hazard is the
 * *rate* and not the depth. So the rate is what is capped: the drive may move
 * by at most [MAX_UNITS_PER_SECOND], which puts a full-range swing at
 * [FULL_RANGE_SECONDS] and an oscillation between the extremes at
 * [MAX_FLASH_HZ] — an order of magnitude under the standard's limit.
 *
 * The cap does the amplitude work by itself, which is why nothing here has to
 * know what a kick is: a signal that alternates faster than the limiter can
 * follow never travels far from where it started, so a fast double-kick barely
 * moves the fog while a long loud passage still brings it all the way up.
 */
internal object FogDrive {
    /** How much of the 0..1 range the drive may cross in one second. */
    const val MAX_UNITS_PER_SECOND = 0.6f

    /** How long the drive needs to cross that range from end to end. */
    const val FULL_RANGE_SECONDS = 1f / MAX_UNITS_PER_SECOND

    /** The fastest full-depth oscillation the cap still permits, in hertz. */
    const val MAX_FLASH_HZ = MAX_UNITS_PER_SECOND / 2f

    /**
     * The drive moved towards [target] by at most what [elapsedSeconds] allows.
     *
     * A non-finite target reads as silence rather than propagating: the fog
     * would otherwise stay stuck at NaN for the rest of the track, and there is
     * no reading behind it worth preserving.
     */
    fun step(current: Float, target: Float, elapsedSeconds: Float): Float {
        val bounded = if (target.isFinite()) target.coerceIn(0f, 1f) else 0f
        if (!current.isFinite()) return bounded
        if (!elapsedSeconds.isFinite() || elapsedSeconds <= 0f) return current
        val room = MAX_UNITS_PER_SECOND * elapsedSeconds
        val delta = bounded - current
        return when {
            delta > room -> current + room
            delta < -room -> current - room
            else -> bounded
        }
    }
}
