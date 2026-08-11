package de.reprise.spike

import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
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

    private const val WIDE_FLOOR = 0.34f
    private const val TIGHT_FLOOR = 0.14f
    private const val FOG_LEVEL_LOW = 0.05f
    private const val FOG_LEVEL_HIGH = 0.70f

    /**
     * How far the haze swells with the signal.
     *
     * Opacity alone was too quiet to notice — against a dark cover the whole
     * range moved the picture by a tenth of a stop. Size is what the eye picks
     * up, so the layers grow with the level as well as brighten.
     */
    private const val SCALE_SWING = 0.14f

    fun breathingSize(baseSizeDp: Float, fogLevel: Float): Float =
        baseSizeDp * (1f + SCALE_SWING * fogLevel.coerceIn(0f, 1f))

    fun wideAlpha(fogLevel: Float, opacity: Float): Float =
        wideOpacity * response(fogLevel, WIDE_FLOOR) * opacity

    fun tightAlpha(fogLevel: Float, opacity: Float): Float =
        tightOpacity * response(fogLevel, TIGHT_FLOOR) * opacity

    private fun response(fogLevel: Float, floor: Float): Float {
        val measuredRange = FOG_LEVEL_HIGH - FOG_LEVEL_LOW
        val normalized = ((fogLevel - FOG_LEVEL_LOW) / measuredRange).coerceIn(0f, 1f)
        return floor + (1f - floor) * normalized
    }

    /**
     * The scrim that keeps the title readable, measured from the cover centre.
     *
     * The vertical scrim below protects the top bar and the transport, and the
     * title sits between them, on bare fog. Against a near-white cover that
     * left white type at 2.32:1. The band is flat across the rows themselves —
     * 150 dp to 252 dp under the centre, where a two-line title and the artist
     * line fall — and fades out beyond them so no edge is visible.
     */
    const val titleScrimFadeTopDp = 128f
    const val titleScrimSolidTopDp = 150f
    const val titleScrimSolidBottomDp = 252f
    const val titleScrimFadeBottomDp = 296f
    const val titleScrimAlpha = 0.58f
}

/** Draws only finished textures; no filtering or artwork decoding enters a frame. */
internal fun DrawScope.drawNowPlayingFog(
    fog: CoverFogBitmap?,
    center: Offset,
    angleA: Float,
    angleB: Float,
    fogLevel: Float,
    opacity: Float,
    rotationsEnabled: Boolean,
) {
    val boundedOpacity = opacity.coerceIn(0f, 1f)
    if (fog != null && boundedOpacity > 0f) {
        drawFogLayer(
            image = fog.wideImage,
            center = center,
            sizeDp = NowPlayingFogSpec.breathingSize(NowPlayingFogSpec.wideSizeDp, fogLevel),
            angle = if (rotationsEnabled) angleA else 0f,
            alpha = NowPlayingFogSpec.wideAlpha(fogLevel, boundedOpacity),
            blendMode = BlendMode.SrcOver,
        )
        drawFogLayer(
            image = fog.tightImage,
            center = center,
            sizeDp = NowPlayingFogSpec.breathingSize(NowPlayingFogSpec.tightSizeDp, fogLevel),
            angle = if (rotationsEnabled) angleB else 0f,
            alpha = NowPlayingFogSpec.tightAlpha(fogLevel, boundedOpacity),
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
    val legibility = fogLegibility(center, opacity)
    drawRect(brush = legibility.vignette)
    drawRect(brush = legibility.edges)
    legibility.titleScrim?.let { scrim -> drawRect(brush = scrim) }
}

/**
 * The legibility gradients, rebuilt only when one of their inputs changed.
 *
 * They are a pure function of the surface, the cover centre and the fade — no
 * scene state reaches them — so one key always describes one set of pixels.
 * Rebuilding them per frame boxed three colour-stop arrays every draw,
 * including the draws where nothing on screen had moved.
 */
private fun DrawScope.fogLegibility(center: Offset, opacity: Float): FogLegibility {
    val key = FogLegibilityKey(center, opacity, size, density)
    val remembered = fogLegibilityMemo
    if (remembered != null && remembered.key == key) return remembered
    val built = FogLegibility(
        key = key,
        vignette = Brush.radialGradient(
            colorStops = arrayOf(
                0f to Color.Transparent,
                0.58f to Color.Black.copy(alpha = 0.08f * opacity),
                1f to Color.Black.copy(alpha = 0.88f * opacity),
            ),
            center = center,
            radius = max(size.width, size.height) * 0.78f,
        ),
        edges = Brush.verticalGradient(
            colorStops = arrayOf(
                0f to Color.Black.copy(alpha = 0.72f * opacity),
                0.28f to Color.Transparent,
                0.68f to Color.Transparent,
                1f to Color.Black.copy(alpha = 0.82f * opacity),
            ),
        ),
        titleScrim = titleScrimBrush(center, opacity),
    )
    fogLegibilityMemo = built
    return built
}

/**
 * Holds the title band down to a readable ground, whatever the cover does.
 *
 * Expressed as four stops of one surface-wide gradient rather than a rect of
 * its own, so the band moves with the cover on any screen height. Null on a
 * surface that leaves the band no room, where nothing is drawn at all.
 */
private fun DrawScope.titleScrimBrush(center: Offset, opacity: Float): Brush? {
    if (size.height <= 0f) return null
    val peak = Color.Black.copy(alpha = NowPlayingFogSpec.titleScrimAlpha * opacity)
    val fadeTop = stopAt(center, NowPlayingFogSpec.titleScrimFadeTopDp)
    val solidTop = stopAt(center, NowPlayingFogSpec.titleScrimSolidTopDp)
    val solidBottom = stopAt(center, NowPlayingFogSpec.titleScrimSolidBottomDp)
    val fadeBottom = stopAt(center, NowPlayingFogSpec.titleScrimFadeBottomDp)
    if (solidBottom <= solidTop) return null
    return Brush.verticalGradient(
        colorStops = arrayOf(
            fadeTop to Color.Transparent,
            solidTop to peak,
            solidBottom to peak,
            fadeBottom to Color.Transparent,
        ),
    )
}

/** One draw's worth of legibility gradients, kept for the next identical draw. */
private class FogLegibility(
    val key: FogLegibilityKey,
    val vignette: Brush,
    val edges: Brush,
    val titleScrim: Brush?,
)

/** Everything the gradients are derived from, and nothing else. */
private data class FogLegibilityKey(
    val center: Offset,
    val opacity: Float,
    val size: Size,
    val density: Float,
)

/** Frames are drawn one at a time, so the last set is all that is worth keeping. */
private var fogLegibilityMemo: FogLegibility? = null

/** One gradient stop, as a fraction of the surface, offset from the cover. */
private fun DrawScope.stopAt(center: Offset, offsetDp: Float): Float =
    ((center.y + offsetDp * density) / size.height).coerceIn(0f, 1f)
