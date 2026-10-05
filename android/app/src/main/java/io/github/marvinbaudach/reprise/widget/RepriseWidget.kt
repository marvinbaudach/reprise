package io.github.marvinbaudach.reprise.widget

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.DpSize
import androidx.core.graphics.scale
import androidx.glance.GlanceId
import androidx.glance.appwidget.GlanceAppWidget
import androidx.glance.appwidget.GlanceAppWidgetReceiver
import androidx.glance.appwidget.SizeMode
import androidx.glance.appwidget.provideContent
import java.io.File
import kotlin.math.ceil
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/**
 * The longest side of any cover bitmap sent to the launcher, in pixels. A
 * bitmap crosses a binder inside RemoteViews, which has a hard size limit
 * shared by every bitmap in the widget.
 */
internal const val WIDGET_COVER_MAX_PX = 192

/**
 * The home-screen widget: wide (cover, title, artist, three buttons) or square
 * (cover with one play/pause button), drawn by whichever size the launcher
 * gives it.
 */
internal class RepriseWidget : GlanceAppWidget() {
    override val sizeMode = SizeMode.Responsive(setOf(WIDE_SIZE, SQUARE_SIZE))

    override suspend fun provideGlance(context: Context, id: GlanceId) {
        val state = WidgetStateStore(context).load()
        val density = context.resources.displayMetrics.density
        val covers = withContext(Dispatchers.IO) { state.artworkPath?.let { decodeCovers(it, density) } }
        provideContent { RepriseWidgetContent(state, covers ?: WidgetCovers.None) }
    }
}

/** The wide placement the launcher offers in its widget picker. */
internal class RepriseWideWidgetReceiver : GlanceAppWidgetReceiver() {
    override val glanceAppWidget = RepriseWidget()
}

/** The square placement the launcher offers in its widget picker. */
internal class RepriseSquareWidgetReceiver : GlanceAppWidgetReceiver() {
    override val glanceAppWidget = RepriseWidget()
}

/** The cover decoded for each layout, each no larger than its slot needs. */
internal data class WidgetCovers(val wide: Bitmap?, val square: Bitmap?) {
    fun forSize(size: DpSize): Bitmap? = if (isWide(size)) wide else square

    companion object {
        val None = WidgetCovers(wide = null, square = null)
    }
}

/** Decodes [path] for both layouts at the screen's [density]. */
internal fun decodeCovers(path: String, density: Float): WidgetCovers = WidgetCovers(
    wide = decodeCover(path, coverPx(COVER_WIDE, density)),
    square = decodeCover(path, coverPx(SQUARE_SIZE.width, density)),
)

/** Pixels a [slot] needs at [density], never more than [WIDGET_COVER_MAX_PX]. */
internal fun coverPx(slot: Dp, density: Float): Int =
    ceil(slot.value * density).toInt().coerceIn(1, WIDGET_COVER_MAX_PX)

/**
 * Decodes [path] no larger than [maxPx] on its longest side: sampled while
 * decoding, so a large cover is never held at full size, and `null` for a file
 * that is gone or no image.
 */
internal fun decodeCover(path: String, maxPx: Int = WIDGET_COVER_MAX_PX): Bitmap? {
    if (!File(path).isFile) return null
    val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
    BitmapFactory.decodeFile(path, bounds)
    if (bounds.outWidth <= 0 || bounds.outHeight <= 0) return null
    var sample = 1
    while (maxOf(bounds.outWidth, bounds.outHeight) / (sample * 2) >= maxPx) sample *= 2
    val decoded = BitmapFactory.decodeFile(
        path,
        BitmapFactory.Options().apply { inSampleSize = sample },
    ) ?: return null
    val longest = maxOf(decoded.width, decoded.height)
    if (longest <= maxPx) return decoded
    val scale = maxPx.toFloat() / longest
    return decoded.scale(
        (decoded.width * scale).toInt().coerceAtLeast(1),
        (decoded.height * scale).toInt().coerceAtLeast(1),
    ).also { if (it !== decoded) decoded.recycle() }
}
