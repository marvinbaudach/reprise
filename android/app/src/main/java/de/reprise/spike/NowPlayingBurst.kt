package de.reprise.spike

import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Canvas
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.FilterQuality
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.CanvasDrawScope
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import de.reprise.spike.scene.CoreShape
import de.reprise.spike.scene.SceneColour
import de.reprise.spike.scene.SceneState
import de.reprise.spike.scene.Transient
import kotlin.math.PI
import kotlin.math.cos
import kotlin.math.hypot
import kotlin.math.max
import kotlin.math.roundToInt
import kotlin.math.sin

internal object NowPlayingBurstSpec {
    const val wedgeCount = 112
    const val coronaStrokeCount = 168
    const val centerHeightFraction = 0.47f
    const val coronaBaseRadiusDp = 86f
    const val coronaBassRadiusDp = 26f
    const val coronaBaseLengthDp = 16f
    const val coronaBandLengthDp = 62f
    const val coronaStrokeWidthDp = 2.1f
    const val coreBaseRadiusDp = 78f
}

internal data class BloomSize(val width: Int, val height: Int)

internal data class BurstHotRay(
    val bandIndex: Int,
    val angleDegrees: Float,
    val excess: Float,
)

internal data class BurstEffects(
    val bloom: Boolean = true,
    val hotRay: Boolean = true,
)

internal fun burstBandIndex(index: Int, itemCount: Int, bandCount: Int): Int {
    if (itemCount <= 0 || bandCount <= 0) return 0
    return (index.coerceIn(0, itemCount - 1) * bandCount / itemCount).coerceIn(0, bandCount - 1)
}

internal fun burstHotRay(transient: Transient?, bandCount: Int): BurstHotRay? {
    if (transient == null || bandCount <= 0) return null
    return BurstHotRay(
        bandIndex = transient.bandIndex.coerceIn(0, bandCount - 1),
        angleDegrees = transient.bandIndex.coerceIn(0, bandCount - 1) * 360f / bandCount,
        excess = transient.excess.coerceIn(0f, 1f),
    )
}

internal fun burstCoreRadii(shape: CoreShape, bass: Float, pointCount: Int): FloatArray =
    FloatArray(pointCount.coerceAtLeast(0)) { index ->
        val theta = index * TWO_PI / pointCount
        shape.radiusAt(theta, NowPlayingBurstSpec.coreBaseRadiusDp, bass)
    }

internal fun burstBloomSize(width: Int, height: Int): BloomSize = BloomSize(
    width = (width / BLOOM_SCALE).coerceAtLeast(1),
    height = (height / BLOOM_SCALE).coerceAtLeast(1),
)

internal fun burstBloomBlurDp(level: Float): Float = 6f + level.coerceIn(0f, 1f) * 16f

internal fun burstBloomOpacity(level: Float): Float =
    level.coerceIn(0f, 1f).let { bounded -> bounded * bounded }

internal class BurstBloomBuffer {
    private var size = BloomSize(0, 0)
    private var bitmap: ImageBitmap? = null

    fun image(width: Int, height: Int): ImageBitmap {
        val next = burstBloomSize(width, height)
        if (next != size || bitmap == null) {
            size = next
            bitmap = ImageBitmap(next.width, next.height)
        }
        return checkNotNull(bitmap)
    }
}

@Composable
internal fun rememberBurstBloomBuffer(): BurstBloomBuffer = remember { BurstBloomBuffer() }

/** Draws the complete burst and its low-resolution bloom into the caller's one Canvas. */
internal fun DrawScope.drawNowPlayingBurst(
    state: SceneState,
    bloomBuffer: BurstBloomBuffer,
    opacity: Float,
    effects: BurstEffects,
) {
    val boundedOpacity = opacity.coerceIn(0f, 1f)
    if (boundedOpacity <= 0f) return
    val center = Offset(size.width / 2f, size.height * NowPlayingBurstSpec.centerHeightFraction)
    drawBurstScene(state, center, boundedOpacity, effects)
    if (effects.bloom && state.level > 0f) {
        drawBurstBloom(state, bloomBuffer, boundedOpacity, effects)
    }
}

private fun DrawScope.drawBurstScene(
    state: SceneState,
    center: Offset,
    opacity: Float,
    effects: BurstEffects,
) {
    drawWedges(state.burstBands, center, opacity)
    drawCorona(state, center, opacity)
    drawCore(state, center, opacity)
    if (effects.hotRay) drawHotRay(state, center, opacity)
}

private fun DrawScope.drawWedges(bands: FloatArray, center: Offset, opacity: Float) {
    val resting = SceneColour.hsl(0f, 0f)
    drawRect(
        color = Color.hsl(resting.hue, resting.saturation, resting.lightness),
        alpha = opacity,
    )
    val radius = hypot(
        max(center.x, size.width - center.x).toDouble(),
        max(center.y, size.height - center.y).toDouble(),
    ).toFloat() * 1.08f
    repeat(NowPlayingBurstSpec.wedgeCount) { index ->
        val band = bands.valueFor(index, NowPlayingBurstSpec.wedgeCount)
        val angle = index * TWO_PI / NowPlayingBurstSpec.wedgeCount - HALF_PI
        val halfStep = TWO_PI / NowPlayingBurstSpec.wedgeCount * 0.53f
        val path = Path().apply {
            moveTo(center.x, center.y)
            lineTo(center.x + cos(angle - halfStep) * radius, center.y + sin(angle - halfStep) * radius)
            lineTo(center.x + cos(angle + halfStep) * radius, center.y + sin(angle + halfStep) * radius)
            close()
        }
        val angleDegrees = index * 360f / NowPlayingBurstSpec.wedgeCount
        val hsl = SceneColour.hsl(angleDegrees, band)
        drawPath(
            path = path,
            color = Color.hsl(hsl.hue, hsl.saturation, hsl.lightness),
            alpha = (0.66f + band * 0.34f) * opacity,
        )
    }
    drawRect(
        brush = Brush.radialGradient(
            colorStops = arrayOf(
                0f to Color.Transparent,
                0.58f to Color.Black.copy(alpha = 0.06f * opacity),
                1f to Color.Black.copy(alpha = 0.58f * opacity),
            ),
            center = center,
            radius = radius,
        ),
    )
}

private fun DrawScope.drawCorona(
    state: SceneState,
    center: Offset,
    opacity: Float,
) {
    val ring = (NowPlayingBurstSpec.coronaBaseRadiusDp + state.bass * NowPlayingBurstSpec.coronaBassRadiusDp) * density
    repeat(NowPlayingBurstSpec.coronaStrokeCount) { index ->
        val band = state.burstBands.valueFor(index, NowPlayingBurstSpec.coronaStrokeCount)
        val response = band * state.level
        val length = (NowPlayingBurstSpec.coronaBaseLengthDp + response * NowPlayingBurstSpec.coronaBandLengthDp) * density
        val angle = index * TWO_PI / NowPlayingBurstSpec.coronaStrokeCount - HALF_PI
        val unit = Offset(cos(angle), sin(angle))
        val start = center + unit * ring
        val end = center + unit * (ring + length)
        val angleDegrees = index * 360f / NowPlayingBurstSpec.coronaStrokeCount
        val hsl = SceneColour.hsl(angleDegrees, band)
        drawLine(
            color = Color.hsl(hsl.hue, hsl.saturation, (hsl.lightness + 0.16f).coerceAtMost(0.74f)),
            start = start,
            end = end,
            strokeWidth = NowPlayingBurstSpec.coronaStrokeWidthDp * density,
            cap = StrokeCap.Round,
            alpha = (0.42f + band * 0.58f) * opacity,
        )
    }
}

private fun DrawScope.drawCore(state: SceneState, center: Offset, opacity: Float) {
    val radii = burstCoreRadii(state.coreShape, state.bass, NowPlayingBurstSpec.coronaStrokeCount)
    val path = Path()
    radii.forEachIndexed { index, radiusDp ->
        val angle = index * TWO_PI / radii.size - HALF_PI
        val point = center + Offset(cos(angle), sin(angle)) * (radiusDp * density)
        if (index == 0) path.moveTo(point.x, point.y) else path.lineTo(point.x, point.y)
    }
    path.close()
    drawPath(path, Color(0xff070710), alpha = opacity)
    drawPath(
        path,
        Color.White.copy(alpha = 0.11f * opacity),
        style = Stroke(width = 1.1f * density),
    )
}

private fun DrawScope.drawHotRay(state: SceneState, center: Offset, opacity: Float) {
    val ray = burstHotRay(state.transient, state.burstBands.size) ?: return
    val radius = hypot(size.width.toDouble(), size.height.toDouble()).toFloat()
    val angle = ray.angleDegrees * PI.toFloat() / 180f - HALF_PI
    val halfStep = TWO_PI / NowPlayingBurstSpec.wedgeCount * 0.9f
    val first = center + Offset(cos(angle - halfStep), sin(angle - halfStep)) * radius
    val second = center + Offset(cos(angle + halfStep), sin(angle + halfStep)) * radius
    val path = Path().apply {
        moveTo(center.x, center.y)
        lineTo(first.x, first.y)
        lineTo(second.x, second.y)
        close()
    }
    val end = center + Offset(cos(angle), sin(angle)) * radius
    val rayAlpha = ray.excess * opacity
    drawPath(
        path,
        brush = Brush.linearGradient(
            colorStops = arrayOf(
                0f to Color.White,
                0.22f to Color(0xffffc2a1),
                1f to Color(0xffff6b16).copy(alpha = 0f),
            ),
            start = center,
            end = end,
        ),
        alpha = rayAlpha,
        blendMode = BlendMode.Screen,
    )
    drawLine(
        color = Color.White,
        start = center,
        end = end,
        strokeWidth = 1.2f * density,
        alpha = rayAlpha,
        cap = StrokeCap.Round,
        blendMode = BlendMode.Screen,
    )
}

private fun DrawScope.drawBurstBloom(
    state: SceneState,
    buffer: BurstBloomBuffer,
    opacity: Float,
    effects: BurstEffects,
) {
    val width = size.width.roundToInt().coerceAtLeast(1)
    val height = size.height.roundToInt().coerceAtLeast(1)
    val image = buffer.image(width, height)
    val bloomSize = burstBloomSize(width, height)
    CanvasDrawScope().draw(
        density = Density(density / BLOOM_SCALE, drawContext.density.fontScale),
        layoutDirection = layoutDirection,
        canvas = Canvas(image),
        size = Size(bloomSize.width.toFloat(), bloomSize.height.toFloat()),
    ) {
        drawRect(Color.Transparent, blendMode = BlendMode.Clear)
        val bloomCenter = Offset(
            bloomSize.width / 2f,
            bloomSize.height * NowPlayingBurstSpec.centerHeightFraction,
        )
        drawBurstScene(state, bloomCenter, 1f, effects)
    }
    val bloomOpacity = burstBloomOpacity(state.level) * opacity
    val blurRadius = burstBloomBlurDp(state.level) * density
    repeat(BLOOM_SAMPLE_COUNT) { index ->
        val angle = index * TWO_PI / BLOOM_SAMPLE_COUNT
        val offset = Offset(cos(angle), sin(angle)) * blurRadius
        drawImage(
            image = image,
            dstOffset = IntOffset(offset.x.roundToInt(), offset.y.roundToInt()),
            dstSize = IntSize(width, height),
            alpha = bloomOpacity / BLOOM_SAMPLE_COUNT,
            blendMode = BlendMode.Screen,
            filterQuality = FilterQuality.Low,
        )
    }
}

private fun FloatArray.valueFor(index: Int, count: Int): Float =
    getOrElse(burstBandIndex(index, count, size)) { 0f }.coerceIn(0f, 1f)

private operator fun Offset.times(scale: Float): Offset = Offset(x * scale, y * scale)

private const val BLOOM_SCALE = 4
private const val BLOOM_SAMPLE_COUNT = 8
private const val TWO_PI = (PI * 2.0).toFloat()
private const val HALF_PI = (PI / 2.0).toFloat()
