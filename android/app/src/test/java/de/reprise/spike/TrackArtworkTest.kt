package de.reprise.spike

import androidx.compose.ui.graphics.ImageBitmap
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit
import org.junit.Assert.assertNull
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
    /**
     * A read that arrives too late is refused, and the refusal stays on the
     * artwork thread.
     *
     * This is the half that is ours. Android ends the process for an exception
     * that escapes any thread, so without the catch in `load` a cover request
     * that lost the race with teardown would not be a lost cover, it would be
     * the crash the refusal exists to prevent. The slot is still answered —
     * with nothing, which is what a track without a readable cover shows
     * anyway.
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
            assertNull("a cover that could not be read is no cover", delivered)
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
