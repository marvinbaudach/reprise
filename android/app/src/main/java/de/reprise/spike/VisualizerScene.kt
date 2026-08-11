package de.reprise.spike

import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.clipRect
import androidx.compose.runtime.staticCompositionLocalOf
import uniffi.reprise_android_ffi.AndroidVisualEngine

internal interface VisualSceneEngine : AutoCloseable {
    fun setAccent(red: Float, green: Float, blue: Float)
    fun setPlaying(playing: Boolean)
    fun noteTrackChanged()
    fun ingestBands(bands: FloatArray)
    fun tick()
    fun scene(width: Float, height: Float): List<Float>
}

internal fun interface VisualSceneEngineFactory {
    fun create(): VisualSceneEngine
}

internal val LocalVisualSceneEngineFactory = staticCompositionLocalOf<VisualSceneEngineFactory> {
    NativeVisualSceneEngineFactory
}

private object NativeVisualSceneEngineFactory : VisualSceneEngineFactory {
    override fun create(): VisualSceneEngine = NativeVisualSceneEngine(AndroidVisualEngine())
}

private class NativeVisualSceneEngine(
    private val native: AndroidVisualEngine,
) : VisualSceneEngine {
    override fun setAccent(red: Float, green: Float, blue: Float) =
        native.setAccent(red, green, blue)

    override fun setPlaying(playing: Boolean) = native.setPlaying(playing)

    override fun noteTrackChanged() = native.noteTrackChanged()

    override fun ingestBands(bands: FloatArray) = native.ingestBands(bands.asList())

    override fun tick() {
        native.tick()
    }

    override fun scene(width: Float, height: Float): List<Float> = native.scene(width, height)

    override fun close() = native.close()
}

/**
 * Draws the flat scene emitted by `AndroidVisualEngine.scene` without rebuilding
 * the portable geometry as a Kotlin object graph.
 *
 * Each record is `[kind, r, g, b, a, width, glow, pointCount, geometry...]`:
 * kind 0 is a four-scalar rectangle, 1 is a polyline with `pointCount` pairs,
 * and 2 is a three-scalar radial glow. Malformed records fail closed.
 */
internal fun DrawScope.drawVisualizerScene(
    buffer: List<Float>,
    bounds: Rect,
    opacity: Float = 1f,
) {
    if (buffer.isEmpty() || bounds.isEmpty || opacity <= 0f) return
    val cursor = FlatSceneCursor(buffer)
    clipRect(bounds.left, bounds.top, bounds.right, bounds.bottom) {
        while (cursor.hasRecord) {
            val header = cursor.header(opacity) ?: return@clipRect
            when (header.kind) {
                RECT_KIND -> drawFlatRect(cursor, bounds, header)
                POLYLINE_KIND -> drawFlatPolyline(cursor, bounds, header)
                RADIAL_GLOW_KIND -> drawFlatRadialGlow(cursor, bounds, header)
                else -> return@clipRect
            }
        }
    }
}

private fun DrawScope.drawFlatRect(
    cursor: FlatSceneCursor,
    bounds: Rect,
    header: FlatShapeHeader,
) {
    if (header.pointCount != RECT_SCALAR_COUNT) return cursor.invalidate()
    if (!cursor.prepareGeometry(RECT_SCALAR_COUNT)) return
    val x = cursor.next()
    val y = cursor.next()
    val width = cursor.next()
    val height = cursor.next()
    drawRect(
        color = header.color,
        topLeft = bounds.topLeft + Offset(x, y),
        size = Size(width.coerceAtLeast(0f), height.coerceAtLeast(0f)),
    )
}

private fun DrawScope.drawFlatPolyline(
    cursor: FlatSceneCursor,
    bounds: Rect,
    header: FlatShapeHeader,
) {
    val scalarCount = header.pointCount.safePairCount() ?: return cursor.invalidate()
    if (!cursor.prepareGeometry(scalarCount)) return
    if (header.pointCount < 2) return cursor.invalidate()
    if (header.width <= 0f) {
        repeat(scalarCount) { cursor.next() }
        return
    }
    val path = Path().apply {
        moveTo(bounds.left + cursor.next(), bounds.top + cursor.next())
        var point = 1
        while (point < header.pointCount) {
            lineTo(
                bounds.left + cursor.next(),
                bounds.top + cursor.next(),
            )
            point += 1
        }
    }
    if (header.glow > 0f) {
        drawPath(
            path = path,
            color = header.color.copy(alpha = header.color.alpha * header.glow),
            style = Stroke(width = header.width * GLOW_STROKE_MULTIPLIER),
        )
    }
    drawPath(path = path, color = header.color, style = Stroke(width = header.width))
}

private fun DrawScope.drawFlatRadialGlow(
    cursor: FlatSceneCursor,
    bounds: Rect,
    header: FlatShapeHeader,
) {
    if (header.pointCount != RADIAL_SCALAR_COUNT) return cursor.invalidate()
    if (!cursor.prepareGeometry(RADIAL_SCALAR_COUNT)) return
    val center = bounds.topLeft + Offset(cursor.next(), cursor.next())
    val radius = cursor.next()
    if (radius <= 0f) return
    drawCircle(
        brush = Brush.radialGradient(
            colors = listOf(header.color, header.color.copy(alpha = 0f)),
            center = center,
            radius = radius,
        ),
        radius = radius,
        center = center,
    )
}

private data class FlatShapeHeader(
    val kind: Int,
    val color: Color,
    val width: Float,
    val glow: Float,
    val pointCount: Int,
)

private class FlatSceneCursor(private val values: List<Float>) {
    private var index = 0
    private var valid = true

    val hasRecord: Boolean
        get() = valid && index < values.size

    fun header(opacity: Float): FlatShapeHeader? {
        if (!hasFinite(HEADER_SCALAR_COUNT)) return invalidateWithNull()
        val kind = values[index].toInt()
        val color = Color(
            red = values[index + 1].coerceIn(0f, 1f),
            green = values[index + 2].coerceIn(0f, 1f),
            blue = values[index + 3].coerceIn(0f, 1f),
            alpha = values[index + 4].coerceIn(0f, 1f) * opacity.coerceIn(0f, 1f),
        )
        val width = values[index + 5].coerceAtLeast(0f)
        val glow = values[index + 6].coerceIn(0f, 1f)
        val rawPointCount = values[index + 7]
        if (rawPointCount < 0f || rawPointCount > MAX_POINT_COUNT || rawPointCount % 1f != 0f) {
            return invalidateWithNull()
        }
        index += HEADER_SCALAR_COUNT
        return FlatShapeHeader(kind, color, width, glow, rawPointCount.toInt())
    }

    fun prepareGeometry(count: Int): Boolean {
        if (hasFinite(count)) return true
        invalidate()
        return false
    }

    fun next(): Float = values[index++]

    fun invalidate() {
        valid = false
    }

    private fun hasFinite(count: Int): Boolean = count >= 0 &&
        index <= values.size - count &&
        (index until index + count).all { values[it].isFinite() }

    private fun <T> invalidateWithNull(): T? {
        invalidate()
        return null
    }
}

private fun Int.safePairCount(): Int? = if (this <= MAX_POINT_COUNT_INT / 2) this * 2 else null

private const val RECT_KIND = 0
private const val POLYLINE_KIND = 1
private const val RADIAL_GLOW_KIND = 2
private const val HEADER_SCALAR_COUNT = 8
private const val RECT_SCALAR_COUNT = 4
private const val RADIAL_SCALAR_COUNT = 3
private const val GLOW_STROKE_MULTIPLIER = 3f
private const val MAX_POINT_COUNT = 1_000_000f
private const val MAX_POINT_COUNT_INT = 1_000_000
