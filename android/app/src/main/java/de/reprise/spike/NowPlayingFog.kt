package de.reprise.spike

import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.FilterQuality
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.rotate
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import kotlin.math.max
import kotlin.math.roundToInt

internal object NowPlayingFogSpec {
    const val wideSizeDp = 620f
    const val tightSizeDp = 470f
    const val wideOpacity = 0.92f
    const val tightOpacity = 0.55f
    const val wideAngleFactor = 0.9f
    const val tightAngleFactor = -0.6f
    const val tightUsesScreenBlend = true
}

/** Draws only finished textures; no filtering or artwork decoding enters a frame. */
internal fun DrawScope.drawNowPlayingFog(
    fog: CoverFogBitmap?,
    center: Offset,
    angleA: Float,
    angleB: Float,
    opacity: Float,
    rotationsEnabled: Boolean,
) {
    val boundedOpacity = opacity.coerceIn(0f, 1f)
    if (fog != null && boundedOpacity > 0f) {
        drawFogLayer(
            image = fog.wideImage,
            center = center,
            sizeDp = NowPlayingFogSpec.wideSizeDp,
            angle = if (rotationsEnabled) angleA else 0f,
            alpha = NowPlayingFogSpec.wideOpacity * boundedOpacity,
            blendMode = BlendMode.SrcOver,
        )
        drawFogLayer(
            image = fog.tightImage,
            center = center,
            sizeDp = NowPlayingFogSpec.tightSizeDp,
            angle = if (rotationsEnabled) angleB else 0f,
            alpha = NowPlayingFogSpec.tightOpacity * boundedOpacity,
            blendMode = BlendMode.Screen,
        )
    }
    drawFogLegibility(center, boundedOpacity)
}

private fun DrawScope.drawFogLayer(
    image: androidx.compose.ui.graphics.ImageBitmap,
    center: Offset,
    sizeDp: Float,
    angle: Float,
    alpha: Float,
    blendMode: BlendMode,
) {
    val side = (sizeDp * density).roundToInt()
    val offset = IntOffset(
        (center.x - side / 2f).roundToInt(),
        (center.y - side / 2f).roundToInt(),
    )
    rotate(angle, center) {
        drawImage(
            image = image,
            dstOffset = offset,
            dstSize = IntSize(side, side),
            alpha = alpha,
            blendMode = blendMode,
            filterQuality = FilterQuality.Low,
        )
    }
}

private fun DrawScope.drawFogLegibility(center: Offset, opacity: Float) {
    if (opacity <= 0f) return
    drawRect(
        brush = Brush.radialGradient(
            colorStops = arrayOf(
                0f to Color.Transparent,
                0.58f to Color.Black.copy(alpha = 0.08f * opacity),
                1f to Color.Black.copy(alpha = 0.88f * opacity),
            ),
            center = center,
            radius = max(size.width, size.height) * 0.78f,
        ),
    )
    drawRect(
        brush = Brush.verticalGradient(
            colorStops = arrayOf(
                0f to Color.Black.copy(alpha = 0.72f * opacity),
                0.28f to Color.Transparent,
                0.68f to Color.Transparent,
                1f to Color.Black.copy(alpha = 0.82f * opacity),
            ),
        ),
    )
}
