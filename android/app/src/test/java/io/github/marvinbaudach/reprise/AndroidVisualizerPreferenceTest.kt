package io.github.marvinbaudach.reprise

import java.util.Collections
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import org.robolectric.RobolectricTestRunner
import uniffi.reprise_android_ffi.AndroidVisualizerChoice
import uniffi.reprise_android_ffi.MusicLibrary
import uniffi.reprise_android_ffi.NoHandle

private const val VISUALIZER_WAIT_SECONDS = 5L

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class AndroidVisualizerPreferenceTest {
    @Test(timeout = 10_000)
    fun writesStayOrderedOffCallerAndAnswerThroughTheHopUntilShutdown() {
        val caller = Thread.currentThread()
        val hops = LinkedBlockingQueue<() -> Unit>()
        val library = RecordingVisualizerLibrary()
        val lane = LibraryWrites(onMainThread = hops::put)
        val preference = AndroidVisualizerPreference(lane) { library }
        val answers = Collections.synchronizedList(mutableListOf<Result<Unit>>())

        preference.setVisualizer(AndroidVisualizerChoice.SPECTRUM, answers::add)
        preference.setVisualizer(AndroidVisualizerChoice.COVER, answers::add)

        assertTrue("teardown must drain both queued preferences", lane.shutdown())
        assertEquals(
            listOf(AndroidVisualizerChoice.SPECTRUM, AndroidVisualizerChoice.COVER),
            library.writes.toList(),
        )
        assertTrue(library.threads.all { it != caller })
        assertTrue("answers must wait for the main-thread hop", answers.isEmpty())

        repeat(2) {
            hops.poll(VISUALIZER_WAIT_SECONDS, TimeUnit.SECONDS)?.invoke()
        }
        assertEquals(2, answers.size)
        assertTrue(answers.all(Result<Unit>::isSuccess))

        preference.setVisualizer(AndroidVisualizerChoice.AMBIENT, answers::add)
        hops.poll(VISUALIZER_WAIT_SECONDS, TimeUnit.SECONDS)?.invoke()

        assertEquals("a stopped lane must reject rather than write", 2, library.writes.size)
        assertTrue(answers.single { it.isFailure }.exceptionOrNull() is IllegalStateException)
    }
}

private class RecordingVisualizerLibrary : MusicLibrary(NoHandle) {
    val writes = Collections.synchronizedList(mutableListOf<AndroidVisualizerChoice>())
    val threads = Collections.synchronizedList(mutableListOf<Thread>())

    override fun setVisualizer(visualizer: AndroidVisualizerChoice) {
        writes += visualizer
        threads += Thread.currentThread()
    }
}
