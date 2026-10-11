package io.github.marvinbaudach.reprise.widget

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.os.Build
import androidx.compose.ui.unit.Dp
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
        val cover = withContext(Dispatchers.IO) { state.artworkPath?.let { decodeWidgetCover(it, density) } }
        provideContent { RepriseWidgetContent(state, cover) }
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

/**
 * Decodes [path] once for every layout, ready to be handed to `RemoteViews`.
 *
 * The widget is responsive, so each redraw composes both layouts, and
 * `RemoteViews` copies a bitmap into shared memory on the main thread once for
 * each layout it appears in unless it is already immutable and shared (a
 * simpleperf run on a Pixel put that copy, `Bitmap_copyAshmem`, at 24-124 ms
 * per redraw). So the cover is one bitmap, at the larger (square) slot's size,
 * that both layouts use by identity, and it is made shared here on the IO
 * thread: the wide layout only scales it down, and the launcher gets one
 * smaller payload instead of two.
 *
 * `SizeMode.Exact` would not avoid the second layout: on Android 12 and later
 * it composes one per size the launcher reports (portrait and landscape), and
 * it recomposes on every resize.
 */
internal fun decodeWidgetCover(path: String, density: Float): Bitmap? =
    decodeCover(path, coverPx(SQUARE_SIZE.width, density))?.let(::shared)

/**
 * [bitmap] as an immutable bitmap in shared memory, which `RemoteViews` hands
 * on without copying (`Bitmap.asShared` exists from Android 12; before that
 * `RemoteViews` makes the copy itself). Recycles [bitmap] when a copy was made.
 */
private fun shared(bitmap: Bitmap): Bitmap {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.S) return bitmap
    val sharedBitmap = bitmap.asShared()
    if (sharedBitmap !== bitmap) bitmap.recycle()
    return sharedBitmap
}

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
