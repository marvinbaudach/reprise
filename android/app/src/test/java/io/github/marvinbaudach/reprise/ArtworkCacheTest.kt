package io.github.marvinbaudach.reprise

import android.graphics.Bitmap
import android.graphics.Color
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.graphics.asImageBitmap
import kotlinx.coroutines.Dispatchers
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import uniffi.reprise_android_ffi.AndroidArtworkSize
import uniffi.reprise_android_ffi.AndroidFallbackCoverColours

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class ArtworkCacheTest {
    @Test
    fun artwork_reports_cache_hits_and_misses() {
        val cache = ArtworkCache()
        val request = request("counted")
        val visual = visual(Color.RED)

        assertNull(cache.artwork(request))
        cache.putArtwork(request, visual)
        assertSame(visual, cache.artwork(request))

        assertEquals(ArtworkCacheStats(hits = 1, misses = 1), cache.artworkStats())
    }

    @Test
    fun seed_artwork_counts_each_cross_size_fallback_as_a_hit() {
        val cache = ArtworkCache()
        val directVisual = visual(Color.RED)
        val resolvedVisual = visual(Color.GREEN)
        val generatedVisual = visual(Color.BLUE)

        cache.putArtwork(request("direct", AndroidArtworkSize.LIST), directVisual)
        cache.putGenerated(
            request("resolved", AndroidArtworkSize.LIST),
            resolvedVisual,
            resolved = true,
        )
        cache.putGenerated(request("generated", AndroidArtworkSize.LIST), generatedVisual)

        assertSame(directVisual, cache.seedArtwork(request("direct")))
        assertSame(resolvedVisual, cache.seedArtwork(request("resolved")))
        assertSame(generatedVisual, cache.seedArtwork(request("generated")))
        assertEquals(ArtworkCacheStats(hits = 3, misses = 3), cache.artworkStats())
    }

    @Test
    fun every_original_screen_row_hits_when_scrolling_back() {
        val cache = ArtworkCache()
        val visibleScreen = (0 until 11).map { request("row-$it", AndroidArtworkSize.LIST) }
        val nextScreen = (11 until 22).map { request("row-$it", AndroidArtworkSize.LIST) }
        val loaded = visual(Color.RED)

        visibleScreen.forEach { cache.loadOnMiss(it, loaded) }
        nextScreen.forEach { cache.loadOnMiss(it, loaded) }
        cache.loadOnMiss(request("playing", AndroidArtworkSize.NOW_PLAYING), loaded)
        val beforeScrollBack = cache.artworkStats()
        visibleScreen.asReversed().forEach { cache.loadOnMiss(it, loaded) }
        val afterScrollBack = cache.artworkStats()

        assertEquals(
            "every row from the original screen must remain cached",
            visibleScreen.size.toLong(),
            afterScrollBack.hits - beforeScrollBack.hits,
        )
        assertEquals(
            "scrolling back to the original screen must not reload artwork",
            0L,
            afterScrollBack.misses - beforeScrollBack.misses,
        )
    }

    @Test
    fun anArtistNamedLikeATrackUriDoesNotInheritItsCover() {
        val cache = ArtworkCache()
        val sharedIdentity = "content://tracks/also-an-artist"
        val track = ArtworkRequest(
            trackUri = sharedIdentity,
            size = AndroidArtworkSize.LIST,
        )
        val artist = ArtworkRequest(
            trackUri = sharedIdentity,
            size = AndroidArtworkSize.LIST,
            kind = ArtworkKind.ARTIST,
            artistName = sharedIdentity,
        )

        cache.putArtwork(track, visual(Color.RED))

        assertNull(cache.artwork(artist))
    }

    @Test
    fun invalidating_album_artwork_drops_track_fallbacks_on_every_shelf_only() {
        val cache = ArtworkCache()
        val listFallback = request("list-fallback", AndroidArtworkSize.LIST)
        val nowPlayingFallback = request("now-playing-fallback")
        val realTrack = request("real", AndroidArtworkSize.LIST)
        val artist = ArtworkRequest(
            trackUri = "artist://Artist",
            size = AndroidArtworkSize.LIST,
            title = "Artist",
            artist = "Artist",
            kind = ArtworkKind.ARTIST,
            artistName = "Artist",
        )
        val listVisual = visual(Color.RED)
        val nowPlayingVisual = visual(Color.GREEN)
        val realVisual = visual(Color.BLUE)
        val artistVisual = visual(Color.YELLOW)
        cache.putGenerated(listFallback, listVisual, resolved = true)
        cache.putGenerated(nowPlayingFallback, nowPlayingVisual, resolved = true)
        cache.putArtwork(realTrack, realVisual)
        cache.putGenerated(artist, artistVisual, resolved = true)

        cache.invalidateAlbumArtwork()

        assertNull(cache.artwork(listFallback))
        assertNull(cache.artwork(nowPlayingFallback))
        assertSame(realVisual, cache.artwork(realTrack))
        assertSame(artistVisual, cache.artwork(artist))
    }

    @Test
    fun artwork_size_lru_evicts_the_oldest_entry_after_its_budget() {
        val cache = ArtworkCache(nowPlayingArtworkCapacity = 2, fogCapacity = 1)
        val first = request("first")
        val second = request("second")
        val third = request("third")
        val firstVisual = visual(Color.RED)
        val secondVisual = visual(Color.GREEN)
        val thirdVisual = visual(Color.BLUE)

        cache.putArtwork(first, firstVisual)
        cache.putArtwork(second, secondVisual)
        assertSame(firstVisual, cache.artwork(first))
        cache.putArtwork(third, thirdVisual)

        assertSame(firstVisual, cache.artwork(first))
        assertNull(cache.artwork(second))
        assertSame(thirdVisual, cache.artwork(third))
    }

    @Test
    fun generated_cover_is_a_dark_gradient_and_never_the_old_teal_accent() {
        val bitmap = fallbackCoverBitmap(
            title = "No local image",
            artist = "An Artist",
            sizePx = 96,
            colours = AndroidFallbackCoverColours(top = 0x5a3322u, bottom = 0x241d35u),
        )

        val top = bitmap.getPixel(4, 4)
        val bottom = bitmap.getPixel(4, bitmap.height - 5)
        assertNotEquals(top, bottom)
        val oldTeal = Color.rgb(0, 150, 136) and 0x00ffffff
        assertNotEquals(oldTeal, top and 0x00ffffff)
        assertNotEquals(oldTeal, bottom and 0x00ffffff)
        assertEquals(255, Color.alpha(top))
    }

    @Test
    fun generated_visuals_are_distinguished_from_resolved_visuals() {
        val resolvedBitmap = Bitmap.createBitmap(4, 4, Bitmap.Config.ARGB_8888)
        val artwork = TrackArtwork(
            resolve = { trackUri, _ -> if (trackUri.endsWith("resolved")) REAL_PATH else null },
            decode = { path -> if (path == REAL_PATH) resolvedBitmap else null },
            fallback = { _, _, _ -> Bitmap.createBitmap(4, 4, Bitmap.Config.ARGB_8888) },
            cache = ArtworkCache(),
            dispatcher = Dispatchers.Unconfined,
            onMainThread = { work -> work() },
        )
        val generatedRequest = request("generated", AndroidArtworkSize.LIST)
        val resolvedRequest = request("resolved", AndroidArtworkSize.LIST)
        val gate = ArtworkRequestGate()
        val admitted = gate.begin(
            resolvedRequest.trackUri,
            resolvedRequest.size,
            resolvedRequest.title,
            resolvedRequest.artist,
        )
        var resolved: ArtworkVisual? = null

        try {
            artwork.loadVisual(admitted, gate) { resolved = it }

            assertTrue(artwork.seedVisual(generatedRequest).generated)
            assertFalse(requireNotNull(resolved).generated)
            assertSame(resolvedBitmap, resolved?.image?.asAndroidBitmap())
        } finally {
            artwork.shutdown()
        }
    }

    @Test
    fun an_invalidated_in_flight_miss_does_not_restore_a_resolved_fallback() {
        val cache = ArtworkCache()
        val request = request("in-flight", AndroidArtworkSize.LIST)
        lateinit var artwork: TrackArtwork
        artwork = TrackArtwork(
            resolve = { _, _ -> artwork.albumCoversChanged(); null },
            fallback = { _, _, _ -> Bitmap.createBitmap(4, 4, Bitmap.Config.ARGB_8888) },
            cache = cache,
            dispatcher = Dispatchers.Unconfined,
            onMainThread = { work -> work() },
        )
        val gate = ArtworkRequestGate()
        val admitted = gate.begin(request.trackUri, request.size, request.title, request.artist)

        try {
            artwork.loadVisual(admitted, gate) {}

            assertNull(cache.artwork(request))
        } finally {
            artwork.shutdown()
        }
    }

    private fun request(
        name: String,
        size: AndroidArtworkSize = AndroidArtworkSize.NOW_PLAYING,
    ) = ArtworkRequest(
        trackUri = "content://tracks/$name",
        size = size,
        title = name,
        artist = "Artist",
    )

    private fun ArtworkCache.loadOnMiss(request: ArtworkRequest, visual: ArtworkVisual) {
        if (artwork(request) == null) putArtwork(request, visual)
    }

    private fun visual(colour: Int): ArtworkVisual {
        val bitmap = Bitmap.createBitmap(4, 4, Bitmap.Config.ARGB_8888).apply { eraseColor(colour) }
        return ArtworkVisual(bitmap.asImageBitmap(), ambientColors = null)
    }

    private companion object {
        const val REAL_PATH = "/covers/resolved.jpg"
    }
}
