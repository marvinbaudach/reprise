package io.github.marvinbaudach.reprise

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.RectF
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlin.math.roundToInt

private const val SHADOW_TEXTURE_SIZE = 480
private const val SHADOW_CONTENT_SIZE = 272
private const val SHADOW_PADDING = 104
private const val SHADOW_OFFSET_Y = 24
private const val SHADOW_CORNER_RADIUS = 18f
private const val SHADOW_ALPHA = 175
private const val SHADOW_BLUR_RADIUS = 32
private const val SHADOW_BLUR_PASSES = 3

/** One process-wide texture; preparation is first entered from the background dispatcher. */
private val sharedCoverShadow = lazy(LazyThreadSafetyMode.SYNCHRONIZED) {
    prepareCoverShadowBitmap()
}

internal data class CoverShadowBitmap(val bitmap: Bitmap) {
    val image: ImageBitmap = bitmap.asImageBitmap()
}

/** Prepares the rounded black source and its blur without relying on hardware-canvas filters. */
internal fun prepareCoverShadowBitmap(): CoverShadowBitmap {
    val source = Bitmap.createBitmap(
        SHADOW_TEXTURE_SIZE,
        SHADOW_TEXTURE_SIZE,
        Bitmap.Config.ARGB_8888,
    )
    val left = SHADOW_PADDING.toFloat()
    val top = (SHADOW_PADDING + SHADOW_OFFSET_Y).toFloat()
    Canvas(source).drawRoundRect(
        RectF(
            left,
            top,
            left + SHADOW_CONTENT_SIZE,
            top + SHADOW_CONTENT_SIZE,
        ),
        SHADOW_CORNER_RADIUS,
        SHADOW_CORNER_RADIUS,
        Paint(Paint.ANTI_ALIAS_FLAG).apply {
            color = Color.argb(SHADOW_ALPHA, 0, 0, 0)
        },
    )
    return CoverShadowBitmap(blurAlpha(source))
}

/** Loads the process texture once; no allocation or filtering happens in a frame draw. */
@Composable
internal fun rememberCoverShadowBitmap(): CoverShadowBitmap? {
    var shadow by remember { mutableStateOf<CoverShadowBitmap?>(null) }
    LaunchedEffect(Unit) {
        shadow = withContext(Dispatchers.Default) { sharedCoverShadow.value }
    }
    return shadow
}

internal fun DrawScope.drawCoverShadow(shadow: CoverShadowBitmap, cover: Rect) {
    val padding = SHADOW_PADDING.dp.toPx()
    val textureSize = SHADOW_TEXTURE_SIZE.dp.toPx().roundToInt()
    drawImage(
        image = shadow.image,
        dstOffset = IntOffset(
            (cover.left - padding).roundToInt(),
            (cover.top - padding).roundToInt(),
        ),
        dstSize = IntSize(textureSize, textureSize),
    )
}

private fun blurAlpha(source: Bitmap): Bitmap {
    val width = source.width
    val height = source.height
    var pixels = IntArray(width * height)
    source.getPixels(pixels, 0, width, 0, 0, width, height)
    repeat(SHADOW_BLUR_PASSES) {
        pixels = blurAlphaHorizontal(pixels, width, height)
        pixels = blurAlphaVertical(pixels, width, height)
    }
    return Bitmap.createBitmap(pixels, width, height, Bitmap.Config.ARGB_8888)
}

private fun blurAlphaHorizontal(source: IntArray, width: Int, height: Int): IntArray {
    val output = IntArray(source.size)
    val count = SHADOW_BLUR_RADIUS * 2 + 1
    for (y in 0 until height) {
        var alpha = 0
        for (offset in -SHADOW_BLUR_RADIUS..SHADOW_BLUR_RADIUS) {
            alpha += source[y * width + offset.coerceIn(0, width - 1)] ushr 24
        }
        for (x in 0 until width) {
            output[y * width + x] = alpha / count shl 24
            val leaving = (x - SHADOW_BLUR_RADIUS).coerceIn(0, width - 1)
            val entering = (x + SHADOW_BLUR_RADIUS + 1).coerceIn(0, width - 1)
            alpha -= source[y * width + leaving] ushr 24
            alpha += source[y * width + entering] ushr 24
        }
    }
    return output
}

private fun blurAlphaVertical(source: IntArray, width: Int, height: Int): IntArray {
    val output = IntArray(source.size)
    val count = SHADOW_BLUR_RADIUS * 2 + 1
    for (x in 0 until width) {
        var alpha = 0
        for (offset in -SHADOW_BLUR_RADIUS..SHADOW_BLUR_RADIUS) {
            alpha += source[offset.coerceIn(0, height - 1) * width + x] ushr 24
        }
        for (y in 0 until height) {
            output[y * width + x] = alpha / count shl 24
            val leaving = (y - SHADOW_BLUR_RADIUS).coerceIn(0, height - 1)
            val entering = (y + SHADOW_BLUR_RADIUS + 1).coerceIn(0, height - 1)
            alpha -= source[leaving * width + x] ushr 24
            alpha += source[entering * width + x] ushr 24
        }
    }
    return output
}
