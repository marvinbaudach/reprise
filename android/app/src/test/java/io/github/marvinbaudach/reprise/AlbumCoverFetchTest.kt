package io.github.marvinbaudach.reprise

import android.graphics.Bitmap
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.reprise_android_ffi.AndroidArtworkSize

private const val WAIT_SECONDS = 5L

/**
 * The gate itself is proven in Rust (B2: `the_gate_off_fetches_nothing`
 * checks off without touching the network). This is the Kotlin half: a list
 * request never reaches `LibrarySession.artworkFetched` at all — gate on or
 * off makes no difference, because `TrackArtwork` never sets `allowFetch`
 * for a list request in the first place (decision 9). Wired through the
 * real `LibrarySession` rather than a bare lambda, so this proves the whole
 * chain `TrackArtwork` -> `LibrarySession.artworkFetched` -> the port, not
 * just `TrackArtwork`'s own routing (see `TrackArtworkTest.aListRowNeverFetchesACover`).
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class AlbumCoverFetchTest {
    @Test
    fun aListRequestNeverReachesArtworkFetchedThroughTheRealLibrarySession() {
        val fetchCalls = AtomicInteger()
        val session = LibrarySession(
            fakeLibrarySessionPort(
                artworkFor = { _, _ -> null },
                artworkFetched = { _, _ ->
                    fetchCalls.incrementAndGet()
                    null
                },
            ),
        )
        val answered = CountDownLatch(1)
        val artwork = TrackArtwork(
            resolve = session::artworkFor,
            resolveAlbumCoverFetched = session::artworkFetched,
            decode = { _ -> null },
            fallback = { _, _, _ -> Bitmap.createBitmap(4, 4, Bitmap.Config.ARGB_8888) },
            onMainThread = { work -> work() },
        )
        val gate = ArtworkRequestGate()
        val request = gate.begin("content://tracks/list-row", AndroidArtworkSize.LIST)

        try {
            artwork.load(request, gate) { answered.countDown() }
            assertTrue(answered.await(WAIT_SECONDS, TimeUnit.SECONDS))
        } finally {
            artwork.shutdown()
        }

        assertEquals(0, fetchCalls.get())
    }
}
