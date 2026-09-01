package io.github.marvinbaudach.reprise

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.LinearGradient
import android.graphics.Paint
import android.graphics.RectF
import android.graphics.Shader
import uniffi.reprise_android_ffi.AndroidFallbackCoverColours
import uniffi.reprise_android_ffi.androidFallbackCoverColours

/** Draws the portable colour decision as a local bitmap; Kotlin owns only pixels. */
internal fun fallbackCoverBitmap(
    title: String,
    artist: String,
    sizePx: Int,
    colours: AndroidFallbackCoverColours = androidFallbackCoverColours(title, artist),
): Bitmap {
    require(sizePx > 0)
    val bitmap = Bitmap.createBitmap(sizePx, sizePx, Bitmap.Config.ARGB_8888)
    val canvas = Canvas(bitmap)
    val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        shader = LinearGradient(
            0f,
            0f,
            0f,
            sizePx.toFloat(),
            colours.top.opaqueArgb(),
            colours.bottom.opaqueArgb(),
            Shader.TileMode.CLAMP,
        )
    }
    canvas.drawRect(0f, 0f, sizePx.toFloat(), sizePx.toFloat(), paint)
    drawRestrainedNote(canvas, sizePx)
    return bitmap
}

private fun drawRestrainedNote(canvas: Canvas, sizePx: Int) {
    val unit = sizePx / 100f
    val note = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        color = Color.argb(42, 255, 255, 255)
    }
    canvas.drawOval(RectF(36f * unit, 60f * unit, 54f * unit, 73f * unit), note)
    canvas.drawRoundRect(
        50f * unit,
        31f * unit,
        56f * unit,
        66f * unit,
        3f * unit,
        3f * unit,
        note,
    )
    canvas.drawRoundRect(
        52f * unit,
        29f * unit,
        72f * unit,
        36f * unit,
        3f * unit,
        3f * unit,
        note,
    )
}

private fun UInt.opaqueArgb(): Int = toInt() or Color.BLACK
