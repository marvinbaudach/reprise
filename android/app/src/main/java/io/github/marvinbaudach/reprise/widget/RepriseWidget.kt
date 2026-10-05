package io.github.marvinbaudach.reprise.widget

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import androidx.glance.GlanceId
import androidx.glance.appwidget.GlanceAppWidget
import androidx.glance.appwidget.GlanceAppWidgetReceiver
import androidx.glance.appwidget.SizeMode
import androidx.glance.appwidget.provideContent
import java.io.File
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/** The longest side of the cover bitmap sent to the launcher, in pixels. */
internal const val WIDGET_COVER_PX = 256

/**
 * The home-screen widget: wide (cover, title, artist, three buttons) or square
 * (cover with one play/pause button), drawn by whichever size the launcher
 * gives it.
 */
internal class RepriseWidget : GlanceAppWidget() {
    override val sizeMode = SizeMode.Responsive(setOf(WIDE_SIZE, SQUARE_SIZE))

    override suspend fun provideGlance(context: Context, id: GlanceId) {
        val state = WidgetStateStore(context).load()
        val cover = withContext(Dispatchers.IO) { state.artworkPath?.let(::decodeCover) }
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
 * Decodes [path] no larger than [WIDGET_COVER_PX]: sampled while decoding, so a
 * large cover is never held at full size, and `null` for a file that is gone
 * or no image.
 */
internal fun decodeCover(path: String): Bitmap? {
    if (!File(path).isFile) return null
    val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
    BitmapFactory.decodeFile(path, bounds)
    if (bounds.outWidth <= 0 || bounds.outHeight <= 0) return null
    var sample = 1
    while (maxOf(bounds.outWidth, bounds.outHeight) / (sample * 2) >= WIDGET_COVER_PX) sample *= 2
    val decoded = BitmapFactory.decodeFile(
        path,
        BitmapFactory.Options().apply { inSampleSize = sample },
    ) ?: return null
    val longest = maxOf(decoded.width, decoded.height)
    if (longest <= WIDGET_COVER_PX) return decoded
    val scale = WIDGET_COVER_PX.toFloat() / longest
    return Bitmap.createScaledBitmap(
        decoded,
        (decoded.width * scale).toInt().coerceAtLeast(1),
        (decoded.height * scale).toInt().coerceAtLeast(1),
        true,
    ).also { if (it !== decoded) decoded.recycle() }
}
