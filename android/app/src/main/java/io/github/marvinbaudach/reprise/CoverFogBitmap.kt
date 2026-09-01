package io.github.marvinbaudach.reprise

import android.graphics.Bitmap
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.toArgb
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlin.math.sqrt

private const val FOG_BITMAP_SIZE = 256
private const val FOG_CONTENT_SIZE = 208
private const val FOG_BLUR_RADIUS_PX = 18
private const val BOX_BLUR_PASSES = 2
private const val RADIAL_FADE_START = 0.62f
private const val SHIMMER_MASK_SOLID = 0.12f
private const val SHIMMER_MASK_CLEAR = 0.68f

/**
 * What one artwork contributes to the scene, prepared once away from the frame.
 *
 * There used to be three textures here, two of them blurred copies of the cover
 * that the fog scaled and counter-rotated behind it. The fog is six colour
 * clouds now and reads [palette] instead, so what is left is the disc the
 * shimmer turns over the cover — and the palette, which is not a texture at all
 * but belongs to the same question: what does this cover look like from far
 * enough away. Both are cached together; the per-panel scene owns their spatial
 * transition, so this value never carries a second cross-fade of its own.
 */
internal data class CoverFogBitmap(
    val disc: Bitmap,
    val palette: OilFilmPalette,
) {
    val discImage: ImageBitmap = disc.asImageBitmap()
}

/**
 * Reads one cover into everything the scene needs from it.
 *
 * This is deliberately Android-bitmap work rather than a Compose modifier:
 * callers run it away from the main thread once per artwork identity, while
 * every later frame only scales and rotates the finished 256 px disc and draws
 * six gradients that were built here.
 */
internal fun prepareCoverFogBitmap(source: Bitmap?, fallbackArgb: Int): CoverFogBitmap =
    CoverFogBitmap(
        disc = applyShimmerMask(prepareFogTexture(source, fallbackArgb)),
        palette = spreadOilFilmClouds(extractOilFilmQuadrants(source)),
    )

/**
 * The cropped, faded and blurred cover the shimmer disc is cut from.
 *
 * Its own step, rather than a stage buried in [prepareCoverFogBitmap], because
 * it is the only place the crop and the radial fade can be looked at before the
 * shimmer mask has eaten the outer two thirds of them.
 */
internal fun prepareFogTexture(source: Bitmap?, fallbackArgb: Int): Bitmap =
    repeatedBoxBlur(cropSquare(source, fallbackArgb), FOG_BLUR_RADIUS_PX)

@Composable
internal fun rememberCoverFogBitmap(
    artwork: ImageBitmap?,
    fallback: Color,
    cache: ArtworkCache = SharedArtworkCache,
): CoverFogBitmap? {
    val fallbackArgb = fallback.toArgb()
    val prepared = remember(artwork, fallbackArgb, cache) {
        mutableStateOf(artwork?.let(cache::fog))
    }
    LaunchedEffect(artwork, fallbackArgb) {
        prepared.value = artwork?.let(cache::fog) ?: withContext(Dispatchers.Default) {
            prepareCoverFogBitmap(artwork?.asAndroidBitmap(), fallbackArgb).also { fog ->
                if (artwork != null) cache.putFog(artwork, fog)
            }
        }
    }
    return prepared.value
}

private fun cropSquare(source: Bitmap?, fallbackArgb: Int): Bitmap {
    val output = Bitmap.createBitmap(FOG_BITMAP_SIZE, FOG_BITMAP_SIZE, Bitmap.Config.ARGB_8888)
    val pixels = IntArray(FOG_BITMAP_SIZE * FOG_BITMAP_SIZE)
    val validSource = source?.takeIf { it.width > 0 && it.height > 0 }
    val sourceSize = validSource?.let { minOf(it.width, it.height) } ?: 1
    val sourceLeft = validSource?.let { (it.width - sourceSize) / 2 } ?: 0
    val sourceTop = validSource?.let { (it.height - sourceSize) / 2 } ?: 0
    val contentOffset = (FOG_BITMAP_SIZE - FOG_CONTENT_SIZE) / 2
    val contentEnd = contentOffset + FOG_CONTENT_SIZE
    for (outputY in 0 until FOG_BITMAP_SIZE) {
        val contentY = (outputY - contentOffset).coerceIn(0, FOG_CONTENT_SIZE - 1)
        val sourceY = sourceTop +
            (contentY * sourceSize / FOG_CONTENT_SIZE).coerceAtMost(sourceSize - 1)
        for (outputX in 0 until FOG_BITMAP_SIZE) {
            val contentX = (outputX - contentOffset).coerceIn(0, FOG_CONTENT_SIZE - 1)
            val sourceX = sourceLeft +
                (contentX * sourceSize / FOG_CONTENT_SIZE).coerceAtMost(sourceSize - 1)
            val colour = validSource?.getPixel(sourceX, sourceY) ?: fallbackArgb
            val insideContent = outputX in contentOffset until contentEnd &&
                outputY in contentOffset until contentEnd
            val alpha = if (insideContent) {
                ((colour ushr 24 and 0xff) * radialAlpha(contentX, contentY)).toInt()
            } else {
                0
            }
            pixels[outputY * FOG_BITMAP_SIZE + outputX] =
                (alpha shl 24) or (colour and 0x00ffffff)
        }
    }
    output.setPixels(pixels, 0, FOG_BITMAP_SIZE, 0, 0, FOG_BITMAP_SIZE, FOG_BITMAP_SIZE)
    return output
}

private fun radialAlpha(x: Int, y: Int): Float {
    val centre = (FOG_CONTENT_SIZE - 1) / 2f
    val radius = FOG_CONTENT_SIZE / 2f
    val dx = x - centre
    val dy = y - centre
    val distance = sqrt(dx * dx + dy * dy) / radius
    if (distance <= RADIAL_FADE_START) return 1f
    if (distance >= 1f) return 0f
    val progress = (distance - RADIAL_FADE_START) / (1f - RADIAL_FADE_START)
    val smoothstep = progress * progress * (3f - 2f * progress)
    return 1f - smoothstep
}

/** Desktop shimmer mask: solid to 12% of radius, linear to clear at 68%. */
internal fun shimmerMaskAlpha(radiusFraction: Float): Float {
    if (radiusFraction <= SHIMMER_MASK_SOLID) return 1f
    if (radiusFraction >= SHIMMER_MASK_CLEAR) return 0f
    return (SHIMMER_MASK_CLEAR - radiusFraction) /
        (SHIMMER_MASK_CLEAR - SHIMMER_MASK_SOLID)
}

private fun applyShimmerMask(source: Bitmap): Bitmap {
    val pixels = IntArray(FOG_BITMAP_SIZE * FOG_BITMAP_SIZE)
    source.getPixels(pixels, 0, FOG_BITMAP_SIZE, 0, 0, FOG_BITMAP_SIZE, FOG_BITMAP_SIZE)
    val centre = (FOG_BITMAP_SIZE - 1) / 2f
    val radius = FOG_BITMAP_SIZE / 2f
    for (y in 0 until FOG_BITMAP_SIZE) {
        val dy = y - centre
        for (x in 0 until FOG_BITMAP_SIZE) {
            val index = y * FOG_BITMAP_SIZE + x
            val pixel = pixels[index]
            val dx = x - centre
            val distance = sqrt(dx * dx + dy * dy) / radius
            val alpha = ((pixel ushr 24 and 0xff) * shimmerMaskAlpha(distance)).toInt()
            pixels[index] = (alpha shl 24) or (pixel and 0x00ffffff)
        }
    }
    return Bitmap.createBitmap(
        pixels,
        FOG_BITMAP_SIZE,
        FOG_BITMAP_SIZE,
        Bitmap.Config.ARGB_8888,
    )
}

private fun repeatedBoxBlur(source: Bitmap, radius: Int): Bitmap {
    var pixels = IntArray(FOG_BITMAP_SIZE * FOG_BITMAP_SIZE)
    source.getPixels(pixels, 0, FOG_BITMAP_SIZE, 0, 0, FOG_BITMAP_SIZE, FOG_BITMAP_SIZE)
    repeat(BOX_BLUR_PASSES) {
        pixels = blurHorizontal(pixels, FOG_BITMAP_SIZE, FOG_BITMAP_SIZE, radius)
        pixels = blurVertical(pixels, FOG_BITMAP_SIZE, FOG_BITMAP_SIZE, radius)
    }
    return Bitmap.createBitmap(
        pixels,
        FOG_BITMAP_SIZE,
        FOG_BITMAP_SIZE,
        Bitmap.Config.ARGB_8888,
    )
}

private fun blurHorizontal(source: IntArray, width: Int, height: Int, radius: Int): IntArray {
    val output = IntArray(source.size)
    for (y in 0 until height) {
        val sums = ChannelSums()
        for (offset in -radius..radius) {
            sums.add(source[y * width + offset.coerceIn(0, width - 1)])
        }
        for (x in 0 until width) {
            output[y * width + x] = sums.average(radius * 2 + 1)
            val leaving = (x - radius).coerceIn(0, width - 1)
            val entering = (x + radius + 1).coerceIn(0, width - 1)
            sums.remove(source[y * width + leaving])
            sums.add(source[y * width + entering])
        }
    }
    return output
}

private fun blurVertical(source: IntArray, width: Int, height: Int, radius: Int): IntArray {
    val output = IntArray(source.size)
    for (x in 0 until width) {
        val sums = ChannelSums()
        for (offset in -radius..radius) {
            sums.add(source[offset.coerceIn(0, height - 1) * width + x])
        }
        for (y in 0 until height) {
            output[y * width + x] = sums.average(radius * 2 + 1)
            val leaving = (y - radius).coerceIn(0, height - 1)
            val entering = (y + radius + 1).coerceIn(0, height - 1)
            sums.remove(source[leaving * width + x])
            sums.add(source[entering * width + x])
        }
    }
    return output
}

private class ChannelSums {
    var alpha = 0L
    var red = 0L
    var green = 0L
    var blue = 0L

    fun add(pixel: Int) {
        alpha += pixel ushr 24 and 0xff
        red += pixel ushr 16 and 0xff
        green += pixel ushr 8 and 0xff
        blue += pixel and 0xff
    }

    fun remove(pixel: Int) {
        alpha -= pixel ushr 24 and 0xff
        red -= pixel ushr 16 and 0xff
        green -= pixel ushr 8 and 0xff
        blue -= pixel and 0xff
    }

    fun average(count: Int): Int = ((alpha / count).toInt() shl 24) or
        ((red / count).toInt() shl 16) or
        ((green / count).toInt() shl 8) or
        (blue / count).toInt()
}
