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
import androidx.compose.ui.graphics.asAndroidBitmap
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
    private val resolveArtistPortraitCached: (String, AndroidArtworkSize) -> String? = { _, _ -> null },
    private val resolveArtistPortraitFetched: (String, AndroidArtworkSize) -> String? = { _, _ -> null },
    private val decode: (String) -> android.graphics.Bitmap? = BitmapFactory::decodeFile,
    private val fallback: (String, String, Int) -> android.graphics.Bitmap = ::fallbackCoverBitmap,
    private val cache: ArtworkCache = SharedArtworkCache,
    private val worker: ExecutorService = singleArtworkThread("reprise-artwork-list"),
    private val fullSizeWorker: ExecutorService = singleArtworkThread("reprise-artwork-full"),
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
    ) = loadVisual(request, gate) { visual -> deliver(visual?.image) }

    /** The full-size lane also derives the three bounded ambient colour fields. */
    fun loadVisual(
        request: ArtworkRequest,
        gate: ArtworkRequestGate,
        deliver: (ArtworkVisual?) -> Unit,
    ) {
        if (!request.refreshesArtistPortrait()) {
            cache.artwork(request)?.let { cached ->
                if (gate.accepts(request)) deliver(cached)
                return
            }
        }
        val lane = when (request.size) {
            AndroidArtworkSize.NOW_PLAYING -> fullSizeWorker
            AndroidArtworkSize.LIST -> worker
            AndroidArtworkSize.ARTIST_DETAIL -> fullSizeWorker
        }
        lane.execute {
            if (!gate.accepts(request)) {
                return@execute
            }
            // Catching here is load-bearing rather than tidy: Android ends the
            // process for an exception that escapes *any* thread, and one of the
            // failures this catches is teardown itself — a read that reaches the
            // library handle after `MainActivity.onDestroy` closed it is refused
            // with `IllegalStateException`. See [shutdown].
            val visual = runCatching { resolveVisual(request) }.getOrElse { error ->
                Log.w(TAG, "Could not read artwork for ${request.trackUri}", error)
                runCatching { generatedVisual(request, resolved = false) }
                    .onFailure { fallbackError ->
                        Log.w(
                            TAG,
                            "Could not generate artwork for ${request.trackUri}",
                            fallbackError,
                        )
                    }
                    .getOrNull()
            }
            if (!gate.accepts(request)) {
                return@execute
            }
            onMainThread {
                if (gate.accepts(request)) {
                    deliver(visual)
                }
            }
        }
    }

    /** Fills both cover and fog LRUs without claiming a visible slot. */
    fun prefetch(request: ArtworkRequest) {
        if (cache.artwork(request) != null) return
        val lane = when (request.size) {
            AndroidArtworkSize.NOW_PLAYING -> fullSizeWorker
            AndroidArtworkSize.LIST -> worker
            AndroidArtworkSize.ARTIST_DETAIL -> fullSizeWorker
        }
        lane.execute {
            val visual = runCatching { resolveVisual(request) }.getOrElse { error ->
                Log.w(TAG, "Could not prefetch artwork for ${request.trackUri}", error)
                runCatching { generatedVisual(request, resolved = false) }
                    .onFailure { fallbackError ->
                        Log.w(
                            TAG,
                            "Could not generate prefetched artwork for ${request.trackUri}",
                            fallbackError,
                        )
                    }
                    .getOrNull()
            }
                ?: return@execute
            if (cache.fog(visual.image) == null) {
                val fog = prepareCoverFogBitmap(
                    visual.image.asAndroidBitmap(),
                    android.graphics.Color.BLACK,
                )
                cache.putFog(visual.image, fog)
            }
        }
    }

    /** Immediate composition seed: cached source first, generated cover otherwise. */
    fun seedVisual(request: ArtworkRequest): ArtworkVisual =
        cache.seedArtwork(request) ?: generatedVisual(request, resolved = false)

    private fun resolveVisual(request: ArtworkRequest): ArtworkVisual {
        if (!request.refreshesArtistPortrait()) {
            cache.artwork(request)?.let { return it }
        }
        val portraitPath = if (request.kind == ArtworkKind.ARTIST) {
            if (request.allowFetch) {
                resolveArtistPortraitFetched(request.artistName, request.size)
            } else {
                resolveArtistPortraitCached(request.artistName, request.size)
            }
        } else {
            null
        }
        val portrait = portraitPath?.let(decode)
        if (portrait != null && request.refreshesArtistPortrait()) {
            cache.invalidateArtistArtwork(request)
        }
        val bitmap = portrait ?: if (request.kind == ArtworkKind.ARTIST) {
            return generatedVisual(request, resolved = true)
        } else {
            resolve(request.trackUri, request.size)?.let(decode)
                ?: return generatedVisual(request, resolved = true)
        }
        return ArtworkVisual(
            image = bitmap.asImageBitmap(),
            ambientColors = if (request.size == AndroidArtworkSize.NOW_PLAYING) {
                extractAmbientArtworkColors(bitmap)
            } else {
                null
            },
        ).also { visual -> cache.putArtwork(request, visual) }
    }

    private fun generatedVisual(request: ArtworkRequest, resolved: Boolean): ArtworkVisual {
        cache.generated(request)?.let { visual ->
            if (resolved) cache.putGenerated(request, visual, resolved = true)
            return visual
        }
        val bitmap = fallback(request.title, request.artist, request.size.fallbackSizePx())
        return ArtworkVisual(
            image = bitmap.asImageBitmap(),
            ambientColors = if (request.size == AndroidArtworkSize.NOW_PLAYING) {
                extractAmbientArtworkColors(bitmap)
            } else {
                null
            },
        ).also { visual -> cache.putGenerated(request, visual, resolved) }
    }

    /**
     * Stops reading covers. It does not wait for the read in progress, and it
     * must not: a cover request that never finishes is a request whose loss
     * costs nothing, so discarding the queue is the whole point of stopping.
     *
     * That is where this parts company with [LibraryWrites.shutdown], which sits
     * beside it in `onDestroy` and briefly drains answered writes. The difference
     * is not how careful the two are, it is what they carry. An answered write has
     * to report exactly once; a cover is a read, and an abandoned read is
     * indistinguishable from a row that scrolled away.
     *
     * `shutdownNow` is that discard and nothing more — it is not what makes the
     * close below it safe. Interrupting a thread that sits inside a native call
     * only raises a flag the call never reads, so it stops nothing already
     * running, here or anywhere.
     *
     * What makes it safe is `MusicLibrary` itself. The generated bindings count
     * a handle's in-flight calls and free the Rust object only when the last one
     * is out, so a `trackArtwork` still running when the handle is closed keeps
     * it alive and frees it itself on the way out; and a read that starts after
     * the close is refused before it reaches native code, which [load] catches.
     * Both halves are pinned by `TrackArtworkTest`, because both are properties
     * of a generated file that a UniFFI upgrade rewrites.
     */
    fun shutdown() {
        worker.shutdownNow()
        fullSizeWorker.shutdownNow()
    }
}

/** No artwork unless an activity provides a reader — previews stay honest. */
internal val LocalTrackArtwork = staticCompositionLocalOf<TrackArtwork?> { null }

internal data class ArtworkVisual(
    val image: ImageBitmap,
    val ambientColors: AmbientArtworkColors?,
)

/**
 * A track's cover, or its deterministic generated cover while one resolves.
 * Tracks without local artwork keep the generated image: nothing is downloaded.
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
    title: String = "",
    artist: String = "",
    size: Int,
    modifier: Modifier = Modifier,
    artworkSize: AndroidArtworkSize = AndroidArtworkSize.LIST,
    shape: Shape? = null,
    decorative: Boolean = false,
) {
    val visual = rememberTrackArtworkVisual(trackUri, artworkSize, title, artist)
    ArtworkCover(visual, size, modifier, shape, decorative)
}

@Composable
internal fun rememberTrackArtworkVisual(
    trackUri: String,
    artworkSize: AndroidArtworkSize,
    title: String = "",
    artist: String = "",
): ArtworkVisual? {
    val artwork = LocalTrackArtwork.current
    val gate = remember { ArtworkRequestGate() }
    val request = remember(trackUri, artworkSize, title, artist) {
        ArtworkRequest(trackUri, artworkSize, title, artist)
    }
    var visual by remember(request, artwork) {
        mutableStateOf(artwork?.seedVisual(request))
    }
    DisposableEffect(request, artwork) {
        val admitted = gate.begin(trackUri, artworkSize, title, artist)
        artwork?.loadVisual(admitted, gate) { loaded -> visual = loaded }
        onDispose { gate.invalidate(admitted) }
    }
    return visual
}

@Composable
internal fun ArtworkCover(
    visual: ArtworkVisual?,
    size: Int,
    modifier: Modifier = Modifier,
    shape: Shape? = null,
    decorative: Boolean = false,
) {
    val cover = visual?.image
    if (cover == null) {
        CoverPlaceholder(size, shape, decorative, modifier)
        return
    }
    Image(
        bitmap = cover,
        contentDescription = if (decorative) null else "Album artwork",
        contentScale = ContentScale.Crop,
        modifier = modifier
            .size(size.dp)
            .clip(shape ?: MaterialTheme.shapes.small),
    )
}

private fun singleArtworkThread(name: String): ExecutorService =
    Executors.newSingleThreadExecutor { runnable -> Thread(runnable, name) }

private fun ArtworkRequest.refreshesArtistPortrait(): Boolean =
    kind == ArtworkKind.ARTIST && allowFetch

private fun AndroidArtworkSize.fallbackSizePx(): Int = when (this) {
    AndroidArtworkSize.LIST -> 168
    AndroidArtworkSize.NOW_PLAYING -> 1_092
    AndroidArtworkSize.ARTIST_DETAIL -> 640
}
