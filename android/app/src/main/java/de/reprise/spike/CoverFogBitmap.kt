package de.reprise.spike

import android.graphics.Bitmap
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.toArgb
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

private const val FOG_BITMAP_SIZE = 256
private const val WIDE_BLUR_RADIUS_PX = 18
private const val TIGHT_BLUR_RADIUS_PX = 10
private const val BOX_BLUR_PASSES = 2

/** The two textures prepared once and only transformed by the frame draw. */
internal data class CoverFogBitmap(
    val wide: Bitmap,
    val tight: Bitmap,
) {
    val wideImage: ImageBitmap = wide.asImageBitmap()
    val tightImage: ImageBitmap = tight.asImageBitmap()
}

/**
 * Crops one cover to a bounded square and prepares both fog textures.
 *
 * This is deliberately Android-bitmap work rather than a Compose modifier:
 * callers run it away from the main thread once per artwork identity, while
 * every later frame only scales and rotates the finished 256 px textures.
 */
internal fun prepareCoverFogBitmap(source: Bitmap?, fallbackArgb: Int): CoverFogBitmap {
    val square = cropSquare(source, fallbackArgb)
    return CoverFogBitmap(
        wide = repeatedBoxBlur(square, WIDE_BLUR_RADIUS_PX),
        tight = repeatedBoxBlur(square, TIGHT_BLUR_RADIUS_PX),
    )
}

@Composable
internal fun rememberCoverFogBitmap(
    artwork: ImageBitmap?,
    fallback: Color,
): CoverFogBitmap? {
    val fallbackArgb = fallback.toArgb()
    var prepared by remember(artwork, fallbackArgb) { mutableStateOf<CoverFogBitmap?>(null) }
    LaunchedEffect(artwork, fallbackArgb) {
        prepared = withContext(Dispatchers.Default) {
            prepareCoverFogBitmap(artwork?.asAndroidBitmap(), fallbackArgb)
        }
    }
    return prepared
}

private fun cropSquare(source: Bitmap?, fallbackArgb: Int): Bitmap {
    val output = Bitmap.createBitmap(FOG_BITMAP_SIZE, FOG_BITMAP_SIZE, Bitmap.Config.ARGB_8888)
    if (source == null || source.width <= 0 || source.height <= 0) {
        output.eraseColor(fallbackArgb)
        return output
    }
    val sourceSize = minOf(source.width, source.height)
    val left = (source.width - sourceSize) / 2
    val top = (source.height - sourceSize) / 2
    val pixels = IntArray(FOG_BITMAP_SIZE * FOG_BITMAP_SIZE)
    for (y in 0 until FOG_BITMAP_SIZE) {
        val sourceY = top + (y * sourceSize / FOG_BITMAP_SIZE).coerceAtMost(sourceSize - 1)
        for (x in 0 until FOG_BITMAP_SIZE) {
            val sourceX = left + (x * sourceSize / FOG_BITMAP_SIZE).coerceAtMost(sourceSize - 1)
            pixels[y * FOG_BITMAP_SIZE + x] = source.getPixel(sourceX, sourceY)
        }
    }
    output.setPixels(pixels, 0, FOG_BITMAP_SIZE, 0, 0, FOG_BITMAP_SIZE, FOG_BITMAP_SIZE)
    return output
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
