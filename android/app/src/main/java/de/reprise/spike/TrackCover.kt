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
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.unit.dp
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors

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
    private val resolve: (String) -> String?,
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
            val image = runCatching { resolve(request.trackUri)?.let(decode) }
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
 */
@Composable
internal fun TrackCover(trackUri: String, size: Int) {
    val artwork = LocalTrackArtwork.current
    val gate = remember { ArtworkRequestGate() }
    var image by remember(trackUri) { mutableStateOf<ImageBitmap?>(null) }
    DisposableEffect(trackUri, artwork) {
        val request = gate.begin(trackUri)
        artwork?.load(request, gate) { loaded -> image = loaded }
        onDispose { gate.invalidate(request) }
    }

    val cover = image
    if (cover == null) {
        CoverPlaceholder(size)
        return
    }
    Image(
        bitmap = cover,
        contentDescription = "Album artwork",
        contentScale = ContentScale.Crop,
        modifier = Modifier
            .size(size.dp)
            .clip(MaterialTheme.shapes.small),
    )
}

private fun singleArtworkThread(): ExecutorService =
    Executors.newSingleThreadExecutor { runnable -> Thread(runnable, "reprise-artwork") }

private fun decodeCachedCover(path: String): ImageBitmap? =
    BitmapFactory.decodeFile(path)?.asImageBitmap()
