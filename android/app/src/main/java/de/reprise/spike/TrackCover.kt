package de.reprise.spike

import android.graphics.BitmapFactory
import android.os.Handler
import android.os.Looper
import android.util.Log
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.size
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.unit.dp
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors
import uniffi.reprise_android_ffi.AndroidArtworkSize

private const val TAG = "RepriseArtwork"

/**
 * Reads one track's cover off the main thread and hands it to the slot that
 * still wants it.
 *
 * Resolving is deliberately *not* part of listing tracks: a window is 200 rows
 * (`#253`), and reading 200 covers inside that query would stall the paging the
 * library screen depends on. One track, one lazy read, exactly like the desktop.
 */
internal class TrackArtwork(
    private val resolve: (String, AndroidArtworkSize) -> String?,
    private val decode: (String) -> ImageBitmap? = ::decodeCachedCover,
    private val worker: ExecutorService = singleArtworkThread(),
    private val onMainThread: (() -> Unit) -> Unit = { work ->
        Handler(Looper.getMainLooper()).post(work)
    },
) {
    /**
     * Resolves `request` and delivers the image on the main thread — but only
     * while `gate` still admits it. The gate is checked before the work as
     * well: during a fling most queued requests belong to rows that are long
     * gone, and reading their covers would delay the ones on screen.
     */
    fun load(
        request: ArtworkRequest,
        gate: ArtworkRequestGate,
        deliver: (ImageBitmap?) -> Unit,
    ) {
        worker.execute {
            if (!gate.accepts(request)) {
                return@execute
            }
            val image = runCatching { resolve(request.trackUri, request.size)?.let(decode) }
                .onFailure { error ->
                    Log.w(TAG, "Could not read artwork for ${request.trackUri}", error)
                }
                .getOrNull()
            if (!gate.accepts(request)) {
                return@execute
            }
            onMainThread {
                if (gate.accepts(request)) {
                    deliver(image)
                }
            }
        }
    }

    fun shutdown() {
        worker.shutdownNow()
    }
}

/** No artwork unless an activity provides a reader — previews stay honest. */
internal val LocalTrackArtwork = staticCompositionLocalOf<TrackArtwork?> { null }

/**
 * A track's cover, or the honest no-artwork symbol until one arrives. Tracks
 * without local artwork keep the symbol: nothing is downloaded here.
 *
 * [decorative] drops the cover's own description for the covers that sit inside
 * a node which already announces the track. A content description anywhere
 * under such a node is merged into it and then wins over the merged title and
 * artist, so a described cover there would be the *only* thing a screen reader
 * reads out.
 */
@Composable
internal fun TrackCover(
    trackUri: String,
    size: Int,
    artworkSize: AndroidArtworkSize = AndroidArtworkSize.LIST,
    shape: Shape? = null,
    decorative: Boolean = false,
) {
    val artwork = LocalTrackArtwork.current
    val gate = remember { ArtworkRequestGate() }
    var image by remember(trackUri, artworkSize) { mutableStateOf<ImageBitmap?>(null) }
    DisposableEffect(trackUri, artwork, artworkSize) {
        val request = gate.begin(trackUri, artworkSize)
        artwork?.load(request, gate) { loaded -> image = loaded }
        onDispose { gate.invalidate(request) }
    }

    val cover = image
    if (cover == null) {
        CoverPlaceholder(size, shape, decorative)
        return
    }
    Image(
        bitmap = cover,
        contentDescription = if (decorative) null else "Album artwork",
        contentScale = ContentScale.Crop,
        modifier = Modifier
            .size(size.dp)
            .clip(shape ?: MaterialTheme.shapes.small),
    )
}

private fun singleArtworkThread(): ExecutorService =
    Executors.newSingleThreadExecutor { runnable -> Thread(runnable, "reprise-artwork") }

private fun decodeCachedCover(path: String): ImageBitmap? =
    BitmapFactory.decodeFile(path)?.asImageBitmap()
