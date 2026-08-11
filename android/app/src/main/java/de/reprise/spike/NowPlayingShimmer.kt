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
    private const val PRESSURE_ALPHA = 0.14f
    private const val SWELL_ALPHA = 0.16f
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

    fun alpha(swell: Float, bassPressure: Float, opacity: Float): Float =
        (
            REST_ALPHA +
                PRESSURE_ALPHA * NowPlayingFogSpec.normalizedPressure(bassPressure) +
                SWELL_ALPHA * NowPlayingFogSpec.normalizedSwell(swell)
            ) * opacity.coerceIn(0f, 1f)
}

/** Draws the already masked texture; no artwork or mask work enters a frame. */
internal fun DrawScope.drawNowPlayingShimmer(
    fog: CoverFogBitmap?,
    center: Offset,
    coverDiameterDp: Float,
    elapsedSeconds: Double,
    swell: Float,
    bassPressure: Float,
    opacity: Float,
    rotationsEnabled: Boolean,
) {
    val prepared = fog ?: return
    val alpha = NowPlayingShimmerSpec.alpha(swell, bassPressure, opacity)
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
