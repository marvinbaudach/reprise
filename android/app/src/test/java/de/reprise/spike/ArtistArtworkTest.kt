package de.reprise.spike

import android.graphics.Bitmap
import android.graphics.Color
import androidx.compose.ui.graphics.asAndroidBitmap
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.reprise_android_ffi.AndroidArtworkSize

private const val ARTIST_WAIT_SECONDS = 5L

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class ArtistArtworkTest {
    @Test
    fun detailStartsWithTheArtworkAlreadyVisibleInTheArtistRow() {
        val rowPortrait = bitmap(Color.RED)
        val detailPortrait = bitmap(Color.BLUE)
        val cache = ArtworkCache()
        val artwork = TrackArtwork(
            resolve = { _, _ -> error("the portrait must win before the album cover") },
            resolveArtistPortraitCached = { _, _ -> "row-portrait" },
            resolveArtistPortraitFetched = { _, _ -> "detail-portrait" },
            decode = { path ->
                when (path) {
                    "row-portrait" -> rowPortrait
                    "detail-portrait" -> detailPortrait
                    else -> null
                }
            },
            fallback = { _, _, _ -> bitmap(Color.MAGENTA) },
            cache = cache,
            onMainThread = { work -> work() },
        )

        try {
            assertSame(
                rowPortrait,
                resolveArtist(artwork, AndroidArtworkSize.LIST, allowFetch = false),
            )
            val detailRequest = ArtworkRequest(
                trackUri = "content://albums/representative",
                size = AndroidArtworkSize.ARTIST_DETAIL,
                title = "The Artist",
                kind = ArtworkKind.ARTIST,
                artistName = "The Artist",
                allowFetch = true,
            )

            assertSame(rowPortrait, artwork.seedVisual(detailRequest).image.asAndroidBitmap())
            assertSame(
                detailPortrait,
                resolveArtist(artwork, AndroidArtworkSize.ARTIST_DETAIL, allowFetch = true),
            )
        } finally {
            artwork.shutdown()
        }
    }

    @Test
    fun aRowResolutionNeverCallsTheFetcher() {
        val cachedCalls = AtomicInteger()
        val fetchCalls = AtomicInteger()
        val portrait = bitmap(Color.RED)
        val artwork = TrackArtwork(
            resolve = { _, _ -> error("a cached portrait must win before the album cover") },
            resolveArtistPortraitCached = { _, _ ->
                cachedCalls.incrementAndGet()
                "cached-portrait"
            },
            resolveArtistPortraitFetched = { _, _ ->
                fetchCalls.incrementAndGet()
                "fetched-portrait"
            },
            decode = { path -> if (path == "cached-portrait") portrait else null },
            cache = ArtworkCache(),
            onMainThread = { work -> work() },
        )

        try {
            val delivered = resolveArtist(artwork, AndroidArtworkSize.LIST, allowFetch = false)

            assertSame(portrait, delivered)
            assertEquals(1, cachedCalls.get())
            assertEquals(0, fetchCalls.get())
        } finally {
            artwork.shutdown()
        }
    }

    @Test
    fun aDetailResolutionCallsTheFetcherExactlyOnce() {
        val fetchCalls = AtomicInteger()
        val portrait = bitmap(Color.BLUE)
        val artwork = TrackArtwork(
            resolve = { _, _ -> error("a fetched portrait must win before the album cover") },
            resolveArtistPortraitFetched = { _, _ ->
                fetchCalls.incrementAndGet()
                "fetched-portrait"
            },
            decode = { path -> if (path == "fetched-portrait") portrait else null },
            cache = ArtworkCache(),
            onMainThread = { work -> work() },
        )

        try {
            val delivered = resolveArtist(
                artwork,
                AndroidArtworkSize.ARTIST_DETAIL,
                allowFetch = true,
            )

            assertSame(portrait, delivered)
            assertEquals(1, fetchCalls.get())
        } finally {
            artwork.shutdown()
        }
    }

    @Test
    fun anArtistWithoutAPortraitFallsBackToTheGeneratedAvatar() {
        val albumCover = bitmap(Color.GREEN)
        val generated = bitmap(Color.MAGENTA)
        val resolveCalls = AtomicInteger()
        val artwork = TrackArtwork(
            resolve = { uri, _ ->
                resolveCalls.incrementAndGet()
                assertEquals("content://albums/representative", uri)
                "album-cover"
            },
            resolveArtistPortraitCached = { _, _ -> null },
            decode = { path -> if (path == "album-cover") albumCover else null },
            fallback = { _, _, _ -> generated },
            cache = ArtworkCache(),
            onMainThread = { work -> work() },
        )

        try {
            assertSame(
                generated,
                resolveArtist(artwork, AndroidArtworkSize.LIST, allowFetch = false),
            )
            assertEquals(0, resolveCalls.get())
        } finally {
            artwork.shutdown()
        }
    }

    @Test
    fun anArtistWithoutEitherGetsTheGeneratedCover() {
        val generated = bitmap(Color.MAGENTA)
        val fallbackCalls = AtomicInteger()
        val resolveCalls = AtomicInteger()
        val artwork = TrackArtwork(
            resolve = { _, _ ->
                resolveCalls.incrementAndGet()
                null
            },
            resolveArtistPortraitCached = { _, _ -> null },
            fallback = { title, _, _ ->
                assertEquals("The Artist", title)
                fallbackCalls.incrementAndGet()
                generated
            },
            cache = ArtworkCache(),
            onMainThread = { work -> work() },
        )

        try {
            assertSame(
                generated,
                resolveArtist(artwork, AndroidArtworkSize.LIST, allowFetch = false),
            )
            assertEquals(1, fallbackCalls.get())
            assertEquals(0, resolveCalls.get())
        } finally {
            artwork.shutdown()
        }
    }

    private fun resolveArtist(
        artwork: TrackArtwork,
        size: AndroidArtworkSize,
        allowFetch: Boolean,
    ): Bitmap? {
        val gate = ArtworkRequestGate()
        val request = gate.begin(
            trackUri = "content://albums/representative",
            size = size,
            title = "The Artist",
            kind = ArtworkKind.ARTIST,
            artistName = "The Artist",
            allowFetch = allowFetch,
        )
        val answered = CountDownLatch(1)
        var delivered: Bitmap? = null

        artwork.loadVisual(request, gate) { visual ->
            delivered = visual?.image?.asAndroidBitmap()
            answered.countDown()
        }

        assertTrue(answered.await(ARTIST_WAIT_SECONDS, TimeUnit.SECONDS))
        return delivered
    }

    private fun bitmap(colour: Int): Bitmap =
        Bitmap.createBitmap(8, 8, Bitmap.Config.ARGB_8888).apply { eraseColor(colour) }
}
