package io.github.marvinbaudach.reprise

import android.util.Log
import io.github.marvinbaudach.reprise.scene.SpectrogramFrames
import java.util.ArrayDeque
import java.util.Collections
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicLong
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.runInterruptible
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.shadows.ShadowLog
import uniffi.reprise_android_ffi.AndroidAnalysisOutcome

/**
 * The loader's part in the spectrum filling while decoding: stale imports are
 * skipped, progress is read fresh every time, and a failing read stays quiet.
 * Robolectric, because the progress path logs through `android.util.Log`.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class TrackAnalysisLoaderSpectrumTest {
    @Before
    fun clearLog() {
        ShadowLog.clear()
    }

    @Test
    fun nav_15e_a_queued_prepare_for_a_track_no_longer_playing_is_skipped() {
        val firstStarted = CountDownLatch(1)
        val releaseFirst = CountDownLatch(1)
        val lastImported = CountDownLatch(1)
        val imported = Collections.synchronizedList(mutableListOf<Long>())
        val mainHops = ArrayDeque<() -> Unit>()
        val loader = TrackAnalysisLoader(
            importAnalysis = { trackId ->
                imported += trackId
                if (trackId == 1L) {
                    firstStarted.countDown()
                    releaseFirst.await()
                }
                if (trackId == 3L) lastImported.countDown()
                AndroidAnalysisOutcome.COMPUTED
            },
            readBars = { _, _ -> null },
            onMainThread = { work -> synchronized(mainHops) { mainHops.add(work) } },
        )

        loader.prepare(1)
        assertTrue("the first import never started", firstStarted.await(2, TimeUnit.SECONDS))
        loader.prepare(2)
        loader.prepare(3)
        releaseFirst.countDown()
        assertTrue("the playing track was never imported", lastImported.await(2, TimeUnit.SECONDS))
        loader.shutdownForTest()
        synchronized(mainHops) { while (mainHops.isNotEmpty()) mainHops.removeFirst().invoke() }

        assertEquals(listOf(1L, 3L), imported.toList())
        assertEquals("a skipped import must not bump the revision", 2L, loader.revision)
    }

    @Test
    fun nav_15e_a_retry_pause_that_ends_for_a_superseded_track_ends_the_loop() {
        val pauseStarted = CountDownLatch(1)
        val releasePause = CountDownLatch(1)
        val secondImported = CountDownLatch(1)
        val firstReimported = CountDownLatch(1)
        val firstImports = AtomicInteger(0)
        val loader = TrackAnalysisLoader(
            importAnalysis = { trackId ->
                if (trackId == 1L && firstImports.incrementAndGet() > 1) firstReimported.countDown()
                if (trackId == 2L) secondImported.countDown()
                AndroidAnalysisOutcome.CANCELLED
            },
            readBars = { _, _ -> null },
            onMainThread = {},
            pauseBetweenAttempts = {
                pauseStarted.countDown()
                runInterruptible(Dispatchers.Default) { releasePause.await() }
            },
        )

        loader.prepare(1)
        assertTrue("the retry pause never started", pauseStarted.await(2, TimeUnit.SECONDS))
        loader.prepare(2)
        assertTrue("the second track was never imported", secondImported.await(2, TimeUnit.SECONDS))
        releasePause.countDown()

        // Shutdown would end the loop too, so the re-import must be ruled out before it.
        assertFalse(
            "the loop imported a track nobody plays any more",
            firstReimported.await(NEGATIVE_PROOF_MS, TimeUnit.MILLISECONDS),
        )
        loader.shutdownForTest()
        assertEquals(1, firstImports.get())
    }

    @Test
    fun nav_15e_a_superseded_import_of_a_track_the_service_left_ends() {
        listOf<Long?>(7L, null).forEach { playing ->
            val imports = AtomicInteger(0)
            val reimported = CountDownLatch(1)
            val loader = TrackAnalysisLoader(
                importAnalysis = {
                    if (imports.incrementAndGet() > 1) reimported.countDown()
                    AndroidAnalysisOutcome.SUPERSEDED
                },
                readBars = { _, _ -> null },
                onMainThread = {},
                pauseBetweenAttempts = {},
                // A skip from the lock screen: no newer prepare reaches a stopped screen.
                playingTrackId = { playing },
            )

            loader.prepare(41)

            // Shutdown would end the loop too, so the re-import must be ruled out before it.
            assertFalse(
                "playing $playing: the abandoned track was decoded again",
                reimported.await(NEGATIVE_PROOF_MS, TimeUnit.MILLISECONDS),
            )
            loader.shutdownForTest()
            assertEquals(1, imports.get())
        }
    }

    @Test
    fun nav_15e_a_superseded_import_of_the_track_still_playing_retries() {
        val imports = AtomicInteger(0)
        val mainHops = ArrayDeque<() -> Unit>()
        val retried = CountDownLatch(1)
        val loader = TrackAnalysisLoader(
            importAnalysis = {
                if (imports.incrementAndGet() == 1) {
                    // A stale supersede reached the track this loader still prepares.
                    AndroidAnalysisOutcome.SUPERSEDED
                } else {
                    retried.countDown()
                    AndroidAnalysisOutcome.COMPUTED
                }
            },
            readBars = { _, _ -> null },
            onMainThread = { work -> synchronized(mainHops) { mainHops.add(work) } },
            pauseBetweenAttempts = {},
            playingTrackId = { 41L },
        )

        loader.prepare(41)
        assertTrue("the playing track was never imported again", retried.await(2, TimeUnit.SECONDS))
        loader.shutdownForTest()

        assertEquals(2, imports.get())
    }

    @Test
    fun nav_15e_a_superseded_import_of_a_track_no_longer_prepared_ends() {
        val imported = Collections.synchronizedList(mutableListOf<Long>())
        val secondImported = CountDownLatch(1)
        lateinit var loader: TrackAnalysisLoader
        loader = TrackAnalysisLoader(
            importAnalysis = { trackId ->
                imported += trackId
                if (trackId == 1L) {
                    loader.prepare(2)
                    AndroidAnalysisOutcome.SUPERSEDED
                } else {
                    secondImported.countDown()
                    AndroidAnalysisOutcome.COMPUTED
                }
            },
            readBars = { _, _ -> null },
            onMainThread = {},
            pauseBetweenAttempts = {},
        )

        loader.prepare(1)
        assertTrue("the next track was never imported", secondImported.await(2, TimeUnit.SECONDS))
        loader.shutdownForTest()

        assertEquals(listOf(1L, 2L), imported.toList())
    }

    @Test
    fun nav_15d_progress_reads_are_never_cached() {
        val reads = AtomicInteger(0)
        val delivered = CountDownLatch(2)
        val answers = Collections.synchronizedList(mutableListOf<PartialTrackAnalysis?>())
        val loader = TrackAnalysisLoader(
            importAnalysis = { AndroidAnalysisOutcome.IMPORTED },
            readBars = { _, _ -> null },
            readProgress = { _, _ ->
                PartialTrackAnalysis(
                    coveredFraction = 0.1f * reads.incrementAndGet(),
                    bars = emptyList(),
                    frames = SpectrogramFrames(2, 10, byteArrayOf()),
                )
            },
            onMainThread = { work -> work() },
        )

        repeat(2) {
            loader.loadProgress(41, 64) { answer ->
                answers += answer
                delivered.countDown()
            }
        }
        assertTrue("the progress answers never arrived", delivered.await(2, TimeUnit.SECONDS))
        loader.shutdownForTest()

        assertEquals(2, reads.get())
        assertEquals(2, answers.map { it?.coveredFraction }.toSet().size)
    }

    @Test
    fun nav_15d_a_progress_read_that_throws_delivers_null() {
        val delivered = CountDownLatch(1)
        val answers = Collections.synchronizedList(mutableListOf<PartialTrackAnalysis?>())
        val loader = TrackAnalysisLoader(
            importAnalysis = { AndroidAnalysisOutcome.IMPORTED },
            readBars = { _, _ -> null },
            readProgress = { _, _ -> error("library closed") },
            onMainThread = { work -> work() },
        )

        loader.loadProgress(41, 64) { answer ->
            answers += answer
            delivered.countDown()
        }
        assertTrue("a failing read never answered", delivered.await(2, TimeUnit.SECONDS))
        loader.shutdownForTest()

        assertNull(answers.single())
    }

    @Test
    fun nav_15d_a_progress_read_that_keeps_failing_warns_once_per_interval() {
        val now = AtomicLong(0L)
        val firstInterval = CountDownLatch(POLLS_PER_INTERVAL)
        val secondInterval = CountDownLatch(POLLS_PER_INTERVAL)
        val loader = TrackAnalysisLoader(
            importAnalysis = { AndroidAnalysisOutcome.IMPORTED },
            readBars = { _, _ -> null },
            readProgress = { _, _ -> error("library closed") },
            onMainThread = { work -> work() },
            clockMs = now::get,
        )

        repeat(POLLS_PER_INTERVAL) { loader.loadProgress(41, 64) { firstInterval.countDown() } }
        assertTrue("the failing reads never answered", firstInterval.await(2, TimeUnit.SECONDS))
        now.set(PROGRESS_WARNING_INTERVAL_MS)
        repeat(POLLS_PER_INTERVAL) { loader.loadProgress(41, 64) { secondInterval.countDown() } }
        assertTrue("the later reads never answered", secondInterval.await(2, TimeUnit.SECONDS))
        loader.shutdownForTest()

        val warnings = ShadowLog.getLogsForTag("RepriseAnalysis").count { it.type == Log.WARN }
        assertEquals("one warning per interval, not one per poll", 2, warnings)
    }

    private companion object {
        const val NEGATIVE_PROOF_MS = 500L
        const val POLLS_PER_INTERVAL = 5
    }
}
