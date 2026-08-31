package io.github.marvinbaudach.reprise

import android.graphics.Bitmap
import android.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.graphics.asImageBitmap
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.RejectedExecutionException
import java.util.concurrent.TimeUnit
import org.junit.Assert.assertNull
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.reprise_android_ffi.AndroidArtworkSize
import uniffi.reprise_android_ffi.MusicLibrary
import uniffi.reprise_android_ffi.NoHandle

/** Long enough that a loaded host is not mistaken for a read that never ran. */
private const val WAIT_SECONDS = 5L

/** What a UniFFI handle says when it is asked for something after being closed. */
private const val DESTROYED = "already been destroyed"

/**
 * Why the artwork thread may be stopped without waiting for it.
 *
 * `TrackArtwork.shutdown` discards its queue and returns, and `onDestroy` closes
 * the library handle immediately afterwards — so the read in progress can still
 * be inside `MusicLibrary.trackArtwork` when the handle it is calling through is
 * closed. That is safe, but for a reason no line of this app states: the
 * generated bindings count a handle's in-flight calls and free the Rust object
 * only when the last one is out, and a call that arrives after the close is
 * refused before it reaches native code.
 *
 * Both halves are properties of a file that is generated, gitignored, and
 * rewritten wholesale by a UniFFI upgrade. These tests are what turns them from
 * something read once into something the gate re-reads.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class TrackArtworkTest {
    @Test
    fun aCachedCoverIsDeliveredSynchronouslyWithoutAnEmptyFrame() {
        val cache = ArtworkCache()
        val request = ArtworkRequest(
            "content://tracks/cached",
            AndroidArtworkSize.NOW_PLAYING,
            title = "Cached",
            artist = "Artist",
        )
        val bitmap = Bitmap.createBitmap(8, 8, Bitmap.Config.ARGB_8888)
        val expected = ArtworkVisual(bitmap.asImageBitmap(), ambientColors = null)
        cache.putArtwork(request, expected)
        val artwork = TrackArtwork(resolve = { _, _ -> error("cache miss") }, cache = cache)
        val gate = ArtworkRequestGate()
        val admitted = gate.begin(
            request.trackUri,
            request.size,
            request.title,
            request.artist,
        )
        var delivered: ArtworkVisual? = null

        try {
            artwork.loadVisual(admitted, gate) { delivered = it }
            assertSame(expected, delivered)
        } finally {
            artwork.shutdown()
        }
    }

    @Test
    fun aTrackWithoutArtworkReceivesAGeneratedCoverInsteadOfTealOrNull() {
        val answered = CountDownLatch(1)
        val generated = Bitmap.createBitmap(8, 8, Bitmap.Config.ARGB_8888).apply {
            eraseColor(Color.rgb(81, 42, 65))
        }
        val artwork = TrackArtwork(
            resolve = { _, _ -> null },
            fallback = { _, _, _ -> generated },
            onMainThread = { work -> work() },
        )
        val gate = ArtworkRequestGate()
        val request = gate.begin(
            "content://tracks/no-cover",
            AndroidArtworkSize.NOW_PLAYING,
            title = "No Cover",
            artist = "Artist",
        )
        var delivered: ArtworkVisual? = null

        try {
            artwork.loadVisual(request, gate) {
                delivered = it
                answered.countDown()
            }
            assertTrue(answered.await(WAIT_SECONDS, TimeUnit.SECONDS))
            assertNotNull(delivered)
            assertNotEquals(Color.rgb(0, 150, 136), delivered?.image?.asAndroidBitmap()?.getPixel(0, 0))
        } finally {
            artwork.shutdown()
        }
    }

    @Test
    fun ambientColorsComeFromBoundedSamplesOfTheAlreadyDecodedArtwork() {
        val bitmap = Bitmap.createBitmap(12, 12, Bitmap.Config.ARGB_8888)
        for (y in 0 until bitmap.height) {
            for (x in 0 until bitmap.width) {
                bitmap.setPixel(
                    x,
                    y,
                    when (x / 4) {
                        0 -> Color.rgb(240, 24, 32)
                        1 -> Color.rgb(16, 220, 80)
                        else -> Color.rgb(32, 72, 232)
                    },
                )
            }
        }

        val fields = extractAmbientArtworkColors(bitmap)

        assertEquals(
            setOf(Color.rgb(240, 24, 32), Color.rgb(16, 220, 80), Color.rgb(32, 72, 232)),
            fields.asList().toSet(),
        )
    }

    /**
     * A full-size sheet request is useful now; queued row work belongs to the
     * list the sheet just covered. It therefore gets a separate serial lane,
     * while each lane keeps deterministic one-at-a-time decoding.
     */
    @Test
    fun nowPlayingArtworkUsesItsOwnLaneInsteadOfWaitingBehindListWork() {
        val listWorker = Executors.newSingleThreadExecutor()
        val fullSizeWorker = Executors.newSingleThreadExecutor()
        val listStarted = CountDownLatch(1)
        val releaseList = CountDownLatch(1)
        val fullSizeAnswered = CountDownLatch(1)
        val artwork = TrackArtwork(
            resolve = { _, size ->
                if (size == AndroidArtworkSize.LIST) {
                    listStarted.countDown()
                    releaseList.await(WAIT_SECONDS, TimeUnit.SECONDS)
                }
                null
            },
            decode = { _ -> null },
            worker = listWorker,
            fullSizeWorker = fullSizeWorker,
            onMainThread = { work -> work() },
        )
        val listGate = ArtworkRequestGate()
        val listRequest = listGate.begin("content://tracks/list", AndroidArtworkSize.LIST)
        val fullSizeGate = ArtworkRequestGate()
        val fullSizeRequest =
            fullSizeGate.begin("content://tracks/now-playing", AndroidArtworkSize.NOW_PLAYING)

        try {
            artwork.load(listRequest, listGate) {}
            assertTrue("the list request must occupy its lane", listStarted.await(1, TimeUnit.SECONDS))

            artwork.load(fullSizeRequest, fullSizeGate) { fullSizeAnswered.countDown() }

            assertTrue(
                "the full-size request must not wait for queued or running list artwork",
                fullSizeAnswered.await(1, TimeUnit.SECONDS),
            )
        } finally {
            releaseList.countDown()
            artwork.shutdown()
        }
    }

    /**
     * Teardown stops *both* lanes.
     *
     * One line of code and nothing pinning it: the full-size lane was added
     * after the list lane, and the next such change is the one that forgets it.
     * A lane that survives `shutdown` is a thread that keeps starting reads
     * while `onDestroy` closes the handle underneath them — which is safe by
     * `MusicLibrary`'s call counter, but pointless work in the moments the app
     * has left, and it is not what this method claims to do.
     */
    @Test
    fun shutdownStopsTheFullSizeLaneAndNotOnlyTheListLane() {
        val listWorker = Executors.newSingleThreadExecutor()
        val fullSizeWorker = Executors.newSingleThreadExecutor()
        val artwork = TrackArtwork(
            resolve = { _, _ -> null },
            decode = { _ -> null },
            worker = listWorker,
            fullSizeWorker = fullSizeWorker,
            onMainThread = { work -> work() },
        )

        artwork.shutdown()

        assertThrows(RejectedExecutionException::class.java) { listWorker.execute {} }
        assertThrows(RejectedExecutionException::class.java) { fullSizeWorker.execute {} }
    }

    /**
     * A read that arrives too late is refused, and the refusal stays on the
     * artwork thread.
     *
     * This is the half that is ours. Android ends the process for an exception
     * that escapes any thread, so without the catch in `load` a cover request
     * that lost the race with teardown would not be a lost cover, it would be
     * the crash the refusal exists to prevent. The slot is still answered with
     * the generated cover, so a teardown race cannot reintroduce an empty frame.
     */
    @Test
    fun aReadThatFindsTheLibraryClosedIsAnsweredRatherThanEndingTheProcess() {
        val escaped = LinkedBlockingQueue<Throwable>()
        val worker = Executors.newSingleThreadExecutor { runnable ->
            Thread(runnable, "artwork-under-test").apply {
                setUncaughtExceptionHandler { _, error -> escaped.put(error) }
            }
        }
        val answered = CountDownLatch(1)
        var delivered: ImageBitmap? = null
        val artwork = TrackArtwork(
            resolve = { _, _ -> throw IllegalStateException("MusicLibrary object has $DESTROYED") },
            decode = { _ -> null },
            fallback = { _, _, _ ->
                Bitmap.createBitmap(4, 4, Bitmap.Config.ARGB_8888)
            },
            cache = ArtworkCache(),
            worker = worker,
            onMainThread = { work -> work() },
        )
        val gate = ArtworkRequestGate()
        val request = gate.begin("content://tracks/830", AndroidArtworkSize.LIST)

        try {
            artwork.load(request, gate) { image ->
                delivered = image
                answered.countDown()
            }

            val reachedTheSlot = answered.await(WAIT_SECONDS, TimeUnit.SECONDS)
            assertNull(
                "nothing may escape the artwork thread: Android ends the process for it",
                escaped.poll(),
            )
            assertTrue("the slot has to be answered even when the read failed", reachedTheSlot)
            assertNotNull("a refused cover read must fall back without an empty frame", delivered)
        } finally {
            artwork.shutdown()
        }
    }

    /**
     * A closed handle refuses the call instead of passing it to native code.
     *
     * This is the half that belongs to UniFFI, and the fake-object constructor
     * is the only way to ask it without a loaded `.so`: it pins that the refusal
     * happens in the call counter, before anything dereferences a handle. What
     * it cannot show is the other half — that a call already in flight defers
     * the free until it returns — because a fake object has nothing to free.
     * That half is read from the counter itself, which this test would also see
     * disappear.
     */
    @Test
    fun aCallMadeAfterTheHandleIsClosedIsRefusedRatherThanPassedToNativeCode() {
        val library = MusicLibrary(NoHandle)
        library.close()

        val refused = assertThrows(IllegalStateException::class.java) {
            library.trackArtwork("content://tracks/830", AndroidArtworkSize.LIST)
        }

        assertTrue(
            "expected the call counter's refusal, got: ${refused.message}",
            refused.message.orEmpty().contains(DESTROYED),
        )
    }
}
