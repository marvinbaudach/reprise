package de.reprise.spike

import android.graphics.Matrix
import android.graphics.Paint
import android.graphics.RadialGradient
import android.graphics.Shader
import android.util.Log
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.RoundRect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.clipRect
import androidx.compose.ui.graphics.drawscope.clipPath
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.graphics.toArgb
import androidx.compose.runtime.staticCompositionLocalOf
import de.reprise.spike.ui.theme.AmbientTrueBlack
import de.reprise.spike.ui.theme.spectralColour
import java.nio.ByteBuffer
import java.nio.ByteOrder
import uniffi.reprise_android_ffi.AndroidVisualEngine

internal interface VisualSceneEngine : AutoCloseable {
    fun setAccent(red: Float, green: Float, blue: Float)
    fun setPlaying(playing: Boolean)
    fun noteTrackChanged()
    fun ingestBands(bands: FloatArray)
    fun hasLiveAudio(): Boolean = false
    fun bassPressure(): VisualBassPressure = VisualBassPressure.SILENT
    fun tick()
    // This list seam keeps Strand-B test engines compatible; real callers must use sceneBytes(),
    // so the default throws instead of turning an accidental production call into an empty frame.
    fun scene(width: Float, height: Float): List<Float> = throw UnsupportedOperationException(
        "VisualSceneEngine.scene() is a test seam; use sceneBytes()",
    )
    fun sceneBytes(width: Float, height: Float): ByteArray = scene(width, height).toFloatBytes()
}

internal data class VisualBassPressure(
    val levelDbfs: Float,
    val baselineDbfs: Float,
    val impact: Float,
    val aura: Float,
    val kick: Float,
    val pressure: Float,
) {
    companion object {
        val SILENT = VisualBassPressure(
            levelDbfs = -140f,
            baselineDbfs = -140f,
            impact = 0f,
            aura = 0f,
            kick = 0f,
            pressure = 0f,
        )
    }
}

internal fun interface VisualSceneEngineFactory {
    fun create(): VisualSceneEngine
}

internal val LocalVisualSceneEngineFactory = staticCompositionLocalOf<VisualSceneEngineFactory> {
    NativeVisualSceneEngineFactory
}

internal object NativeVisualSceneEngineFactory : VisualSceneEngineFactory {
    override fun create(): VisualSceneEngine = NativeVisualSceneEngine(AndroidVisualEngine())
}

internal class NativeVisualSceneEngine(
    private val native: AndroidVisualEngine,
) : VisualSceneEngine, LivePcmConsumer {
    private var sceneCallsUntilCounterLog = 0

    override fun setAccent(red: Float, green: Float, blue: Float) =
        native.setAccent(red, green, blue)

    override fun setPlaying(playing: Boolean) = native.setPlaying(playing)

    override fun noteTrackChanged() = native.noteTrackChanged()

    override fun ingestBands(bands: FloatArray) = native.ingestBands(bands.asList())

    override fun setPlaybackIntent(playbackIntended: Boolean) =
        native.setPlaybackIntended(playbackIntended)

    override fun ingestPcm16(
        bytes: ByteArray,
        byteCount: Int,
        sampleRateHz: Int,
        channelCount: Int,
    ) {
        native.ingestPcmI16(
            bytes,
            byteCount.toUInt(),
            sampleRateHz.toUInt(),
            channelCount.toUInt(),
        )
    }

    override fun resetAudioStream() = native.resetAudioStream()

    override fun resetAudioHistory() = native.resetAudioHistory()

    override fun hasLiveAudio(): Boolean = native.hasLiveAudio()

    override fun bassPressure(): VisualBassPressure = native.bassPressure().let { pressure ->
        VisualBassPressure(
            levelDbfs = pressure.levelDbfs,
            baselineDbfs = pressure.baselineDbfs,
            impact = pressure.impact,
            aura = pressure.aura,
            kick = pressure.kick,
            pressure = pressure.pressure,
        )
    }

    override fun tick() {
        native.tick()
    }

    override fun sceneBytes(width: Float, height: Float): ByteArray =
        native.scene(width, height).also { logDroppedAudioFrames() }

    private fun logDroppedAudioFrames() {
        if (sceneCallsUntilCounterLog > 0) {
            sceneCallsUntilCounterLog -= 1
            return
        }
        sceneCallsUntilCounterLog = COUNTER_LOG_EVERY_SCENE_CALLS - 1
        val dropped = native.droppedAudioFrames()
        Log.i(VISUAL_SCENE_LOG_TAG, "dropped_audio_frames=$dropped")
    }

    override fun close() = native.close()
}

internal fun DrawScope.drawPlayedVisualizer(
    buffer: ByteArray,
    center: Offset,
    side: Float,
    radius: Float,
    shadow: CoverShadowBitmap?,
    opacity: Float = 1f,
) {
    val rect = playedCoverRect(center, side)
    shadow?.let { drawCoverShadow(it, rect) }
    if (opacity <= 0f) return
    val safeOpacity = opacity.coerceIn(0f, 1f)
    val path = Path().apply { addRoundRect(RoundRect(rect, CornerRadius(radius))) }
    clipPath(path) {
        drawRect(
            AmbientTrueBlack.copy(alpha = safeOpacity),
            topLeft = rect.topLeft,
            size = rect.size,
        )
        drawVisualizerScene(buffer = buffer, bounds = rect, opacity = safeOpacity)
    }
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
    buffer: ByteArray,
    bounds: Rect,
    opacity: Float = 1f,
) {
    if (buffer.isEmpty() || bounds.isEmpty || opacity <= 0f) return
    val cursor = FlatSceneCursor(buffer)
    val polylinePath = Path()
    val radialGlowPainter = RADIAL_GLOW_PAINTER.get()
    clipRect(bounds.left, bounds.top, bounds.right, bounds.bottom) {
        while (cursor.hasRecord) {
            val header = cursor.header(opacity) ?: return@clipRect
            when (header.kind) {
                RECT_KIND -> drawFlatRect(cursor, bounds, header)
                POLYLINE_KIND -> drawFlatPolyline(cursor, bounds, header, polylinePath)
                RADIAL_GLOW_KIND -> drawFlatRadialGlow(
                    cursor,
                    bounds,
                    header,
                    radialGlowPainter,
                )
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
    path: Path,
) {
    val scalarCount = header.pointCount.safePairCount() ?: return cursor.invalidate()
    if (!cursor.prepareGeometry(scalarCount)) return
    if (header.pointCount < 2) return cursor.invalidate()
    if (header.width <= 0f) {
        repeat(scalarCount) { cursor.next() }
        return
    }
    path.reset()
    path.moveTo(bounds.left + cursor.next(), bounds.top + cursor.next())
    var point = 1
    while (point < header.pointCount) {
        path.lineTo(
            bounds.left + cursor.next(),
            bounds.top + cursor.next(),
        )
        point += 1
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
    painter: RadialGlowPainter,
) {
    if (header.pointCount != RADIAL_SCALAR_COUNT) return cursor.invalidate()
    if (!cursor.prepareGeometry(RADIAL_SCALAR_COUNT)) return
    val center = bounds.topLeft + Offset(cursor.next(), cursor.next())
    val radius = cursor.next()
    if (radius <= 0f) return
    painter.draw(drawContext.canvas.nativeCanvas, center, radius, header.color)
}

private class RadialGlowPainter {
    private val paint = Paint(Paint.ANTI_ALIAS_FLAG)
    private val matrix = Matrix()
    // Insertion-order eviction is safe while bars is the only mode emitting radial glows: its RGB
    // comes only from the bar index, for at most BAR_COUNT (currently 64) bit-identical keys. A future
    // mode with continuously varying glow colours would make the eviction policy matter.
    private val shaders = LinkedHashMap<Int, RadialGradient>()

    fun draw(canvas: android.graphics.Canvas, center: Offset, radius: Float, color: Color) {
        val argb = color.toArgb()
        val rgb = argb and RGB_MASK
        val shader = shaderFor(rgb)
        matrix.setScale(radius, radius)
        matrix.postTranslate(center.x, center.y)
        shader.setLocalMatrix(matrix)
        paint.shader = shader
        paint.alpha = argb ushr ALPHA_SHIFT
        canvas.drawCircle(center.x, center.y, radius, paint)
    }

    private fun shaderFor(rgb: Int): RadialGradient {
        shaders[rgb]?.let { return it }
        if (shaders.size == MAX_CACHED_GLOW_COLOURS) {
            val oldest = shaders.entries.iterator()
            oldest.next()
            oldest.remove()
        }
        return RadialGradient(
            0f,
            0f,
            1f,
            rgb or OPAQUE_ALPHA,
            rgb,
            Shader.TileMode.CLAMP,
        ).also { shaders[rgb] = it }
    }
}

private data class FlatShapeHeader(
    val kind: Int,
    val color: Color,
    val width: Float,
    val glow: Float,
    val pointCount: Int,
)

private class FlatSceneCursor(bytes: ByteArray) {
    private val values = ByteBuffer.wrap(bytes)
        .order(ByteOrder.LITTLE_ENDIAN)
        .asFloatBuffer()
    private var index = 0
    private var valid = bytes.size % Float.SIZE_BYTES == 0

    val hasRecord: Boolean
        get() = valid && index < values.limit()

    fun header(opacity: Float): FlatShapeHeader? {
        if (!hasFinite(HEADER_SCALAR_COUNT)) return invalidateWithNull()
        val kind = values.get(index).toInt()
        val color = spectralColour(
            red = values.get(index + 1),
            green = values.get(index + 2),
            blue = values.get(index + 3),
            alpha = values.get(index + 4).coerceIn(0f, 1f) * opacity.coerceIn(0f, 1f),
        )
        val width = values.get(index + 5).coerceAtLeast(0f)
        val glow = values.get(index + 6).coerceIn(0f, 1f)
        val rawPointCount = values.get(index + 7)
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

    fun next(): Float = values.get(index++)

    fun invalidate() {
        valid = false
    }

    private fun hasFinite(count: Int): Boolean = count >= 0 &&
        index <= values.limit() - count &&
        (index until index + count).all { values.get(it).isFinite() }

    private fun <T> invalidateWithNull(): T? {
        invalidate()
        return null
    }
}

private fun List<Float>.toFloatBytes(): ByteArray =
    ByteBuffer.allocate(size * Float.SIZE_BYTES)
        .order(ByteOrder.LITTLE_ENDIAN)
        .apply { this@toFloatBytes.forEach(::putFloat) }
        .array()

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
private const val COUNTER_LOG_EVERY_SCENE_CALLS = 12
private const val VISUAL_SCENE_LOG_TAG = "RepriseVisualScene"
private const val MAX_CACHED_GLOW_COLOURS = 128
private const val RGB_MASK = 0x00ffffff
private const val OPAQUE_ALPHA = -0x1000000
private const val ALPHA_SHIFT = 24
// Intentionally process-lifetime per thread, not remember-scoped: reuse is the point. The painter
// owns no bitmap or native buffer beyond its capped shader cache, so this lifetime is not a leak.
private val RADIAL_GLOW_PAINTER = ThreadLocal.withInitial(::RadialGlowPainter)
