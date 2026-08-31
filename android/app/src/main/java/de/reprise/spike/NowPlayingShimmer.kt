package de.reprise.spike

import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.FilterQuality
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.rotate
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import kotlin.math.floor
import kotlin.math.roundToInt

/** Geometry and signal response of the cover disc that turns behind the cover. */
internal object NowPlayingShimmerSpec {
    private const val REST_ALPHA = 0.34f

    /**
     * What the level carries, after the kick's share was folded into it.
     *
     * The disc used to answer the kick with 0.14 of its own, on top of 0.16 for
     * the level. Dropping that term outright would have dimmed the disc at its
     * peak, so the weight moved instead of vanishing: the same brightest state
     * is still reachable, it is now reached by a loud passage rather than by a
     * beat, and the level driving it is rate-capped upstream.
     */
    private const val SWELL_ALPHA = 0.30f

    /**
     * The phone fog spends roughly three times the alpha of the desktop bloom below this
     * disc. Full desktop shimmer alpha lifted a measured near-white surround by 18 brightness
     * levels, so one factor reduces every term while preserving their tuned proportions.
     */
    const val OVER_FOG_SCALE = 1f / 3f

    /**
     * The disc over the artist portrait, which keeps the desktop's own terms.
     *
     * A film drifts below it there too, so the reduction above is not wrong about what
     * is underneath. What differs is everything the played view puts on top: that one
     * follows its film with a surface-wide vignette and two edge gradients, and the disc
     * is read through them. The artist page draws no scrims — the album rows would go
     * with them — so the same alpha arrives on screen at a third of the weight. Measured
     * on a real phone at [OVER_FOG_SCALE] it read as a faint tint, near-white artwork
     * included.
     */
    const val ON_BARE_SURFACE_SCALE = 1f

    private const val TURN_SECONDS = 60.0
    private const val DEGREES_PER_TURN = 360.0
    private const val DESKTOP_DIAMETER_TO_COVER_RATIO = 520f / 168f

    /**
     * Keeps the desktop's 520 px disc over its 168 px cover. The radial mask was tuned
     * against this ratio, so the ratio rather than one phone dp size is the geometry contract.
     */
    fun diameterDp(coverDiameterDp: Float): Float =
        coverDiameterDp * DESKTOP_DIAMETER_TO_COVER_RATIO

    fun angleDegrees(elapsedSeconds: Double): Float {
        val turns = elapsedSeconds / TURN_SECONDS
        val wrappedTurns = turns - floor(turns)
        return (DEGREES_PER_TURN * wrappedTurns).toFloat()
    }

    fun alpha(swell: Float, opacity: Float, scale: Float = OVER_FOG_SCALE): Float =
        (
            REST_ALPHA + SWELL_ALPHA * NowPlayingFogSpec.normalizedSwell(swell)
            ) * scale * opacity.coerceIn(0f, 1f)
}

/** Draws the already masked texture; no artwork or mask work enters a frame. */
internal fun DrawScope.drawNowPlayingShimmer(
    fog: CoverFogBitmap?,
    center: Offset,
    coverDiameterDp: Float,
    elapsedSeconds: Double,
    swell: Float,
    opacity: Float,
    rotationsEnabled: Boolean,
    alphaScale: Float = NowPlayingShimmerSpec.OVER_FOG_SCALE,
) {
    val prepared = fog ?: return
    val alpha = NowPlayingShimmerSpec.alpha(swell, opacity, alphaScale)
    if (alpha <= 0f) return
    val side = (NowPlayingShimmerSpec.diameterDp(coverDiameterDp) * density).roundToInt()
    val offset = IntOffset(
        (center.x - side / 2f).roundToInt(),
        (center.y - side / 2f).roundToInt(),
    )
    val angle = if (rotationsEnabled) {
        NowPlayingShimmerSpec.angleDegrees(elapsedSeconds)
    } else {
        0f
    }
    rotate(angle, center) {
        drawImage(
            image = prepared.discImage,
            dstOffset = offset,
            dstSize = IntSize(side, side),
            alpha = alpha,
            filterQuality = FilterQuality.Low,
        )
    }
}
