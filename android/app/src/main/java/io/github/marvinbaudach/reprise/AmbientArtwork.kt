package io.github.marvinbaudach.reprise

import android.graphics.Bitmap

private const val OPAQUE_ALPHA = 255
private const val VISIBLE_ALPHA_THRESHOLD = 128

private fun alpha(argb: Int): Int = argb ushr 24 and 0xff

private fun redChannel(argb: Int): Int = argb ushr 16 and 0xff

private fun greenChannel(argb: Int): Int = argb ushr 8 and 0xff

private fun blueChannel(argb: Int): Int = argb and 0xff

private fun opaque(red: Int, green: Int, blue: Int): Int =
    (OPAQUE_ALPHA shl 24) or (red shl 16) or (green shl 8) or blue

private val opaqueBlack = opaque(0, 0, 0)

/** Three cover-derived ARGB fields; Compose colours remain owned by the theme. */
internal data class AmbientArtworkColors(
    val first: Int,
    val second: Int,
    val third: Int,
) {
    fun asList(): List<Int> = listOf(first, second, third)
}

private data class ColorBucket(
    var count: Int = 0,
    var red: Long = 0,
    var green: Long = 0,
    var blue: Long = 0,
) {
    fun add(argb: Int) {
        count += 1
        red += redChannel(argb)
        green += greenChannel(argb)
        blue += blueChannel(argb)
    }

    fun average(): Int = opaque(
        (red / count).toInt(),
        (green / count).toInt(),
        (blue / count).toInt(),
    )
}

/**
 * Samples at most 12 x 12 pixels and groups them into 4-bit RGB buckets.
 *
 * The full cover has already been decoded on the artwork worker. Sampling that
 * bitmap avoids both a second decode and a Palette dependency, while the fixed
 * 144-pixel ceiling keeps a track change independent of cover resolution.
 */
internal fun extractAmbientArtworkColors(bitmap: Bitmap): AmbientArtworkColors {
    if (bitmap.width <= 0 || bitmap.height <= 0) {
        return AmbientArtworkColors(opaqueBlack, opaqueBlack, opaqueBlack)
    }
    val sampleColumns = minOf(12, bitmap.width)
    val sampleRows = minOf(12, bitmap.height)
    val buckets = linkedMapOf<Int, ColorBucket>()
    for (sampleY in 0 until sampleRows) {
        val y = sampleCoordinate(sampleY, sampleRows, bitmap.height)
        for (sampleX in 0 until sampleColumns) {
            val x = sampleCoordinate(sampleX, sampleColumns, bitmap.width)
            val argb = bitmap.getPixel(x, y)
            if (alpha(argb) < VISIBLE_ALPHA_THRESHOLD) continue
            val key = (redChannel(argb) shr 4 shl 8) or
                (greenChannel(argb) shr 4 shl 4) or
                (blueChannel(argb) shr 4)
            buckets.getOrPut(key, ::ColorBucket).add(argb)
        }
    }
    val selected = buckets.entries
        .sortedWith(compareByDescending<Map.Entry<Int, ColorBucket>> { it.value.count }.thenBy { it.key })
        .take(3)
        .map { it.value.average() }
        .toMutableList()
    if (selected.isEmpty()) selected += opaqueBlack
    while (selected.size < 3) selected += selected.last()
    return AmbientArtworkColors(selected[0], selected[1], selected[2])
}

private fun sampleCoordinate(sample: Int, sampleCount: Int, size: Int): Int =
    if (sampleCount <= 1) 0 else sample * (size - 1) / (sampleCount - 1)
