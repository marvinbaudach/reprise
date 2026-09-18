package io.github.marvinbaudach.reprise

import java.util.ArrayDeque
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicReference
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.reprise_android_ffi.AndroidAnalysisOutcome
import uniffi.reprise_android_ffi.AndroidTrackSpectrogram

class TrackAnalysisLoaderTest {
    @Test
    fun finishedBarsAreWarmAcrossRecompositionWithoutAnotherRead() {
        val mainHops = ArrayDeque<() -> Unit>()
        val readStarted = CountDownLatch(1)
        val releaseRead = CountDownLatch(1)
        val answerQueued = CountDownLatch(1)
        var reads = 0
        val expected = listOf(SpectralBar(false, 0.75f, 0.1, 0.2, 0.3))
        val loader = TrackAnalysisLoader(
            importAnalysis = { AndroidAnalysisOutcome.IMPORTED },
            readBars = { _, _ ->
                reads += 1
                readStarted.countDown()
                releaseRead.await()
                expected
            },
            onMainThread = { work ->
                mainHops.add(work)
                answerQueued.countDown()
            },
        )

        loader.loadBars(41, 64) {}
        assertTrue("the bar read never started", readStarted.await(2, TimeUnit.SECONDS))
        cancelDrainWhileWorkIsStarted(loader, releaseRead)
        assertTrue("the bar answer was never queued", answerQueued.await(2, TimeUnit.SECONDS))
        while (mainHops.isNotEmpty()) mainHops.removeFirst().invoke()

        var delivered: List<SpectralBar>? = null
        loader.loadBars(41, 64) { delivered = it }

        assertEquals(expected, delivered)
        assertTrue(loader.warmth(41).bars)
        assertEquals(1, reads)
    }

    @Test
    fun aPrefetchedSpectrogramIsWarmAcrossRecompositionWithoutAnotherRead() {
        val mainHops = ArrayDeque<() -> Unit>()
        val readStarted = CountDownLatch(1)
        val releaseRead = CountDownLatch(1)
        val answerQueued = CountDownLatch(1)
        var reads = 0
        val expected = AndroidTrackSpectrogram(2u, 10u, byteArrayOf(1, 2))
        val loader = TrackAnalysisLoader(
            importAnalysis = { AndroidAnalysisOutcome.IMPORTED },
            readBars = { _, _ -> null },
            readSpectrogram = {
                reads += 1
                readStarted.countDown()
                releaseRead.await()
                expected
            },
            onMainThread = { work ->
                mainHops.add(work)
                answerQueued.countDown()
            },
        )

        loader.prefetch(listOf(41))
        assertTrue("the prefetch never reached FFI", readStarted.await(2, TimeUnit.SECONDS))
        cancelDrainWhileWorkIsStarted(loader, releaseRead)
        assertTrue("the spectrogram answer was never queued", answerQueued.await(2, TimeUnit.SECONDS))
        while (mainHops.isNotEmpty()) mainHops.removeFirst().invoke()

        var delivered = false
        loader.loadSpectrogram(41) { delivered = true }

        assertTrue(delivered)
        assertTrue(loader.warmth(41).spectrogram)
        assertEquals(1, reads)
    }

    @Test
    fun preparingAPrefetchedTrackKeepsItsPositiveCacheHitSynchronous() {
        val mainHops = ArrayDeque<() -> Unit>()
        val answersQueued = CountDownLatch(2)
        val importStarted = CountDownLatch(1)
        val releaseImport = CountDownLatch(1)
        var reads = 0
        val expected = AndroidTrackSpectrogram(2u, 10u, byteArrayOf(1, 2))
        val loader = TrackAnalysisLoader(
            importAnalysis = {
                importStarted.countDown()
                releaseImport.await()
                AndroidAnalysisOutcome.IMPORTED
            },
            readBars = { _, _ -> null },
            readSpectrogram = {
                reads += 1
                expected
            },
            onMainThread = { work ->
                mainHops.add(work)
                answersQueued.countDown()
            },
        )

        try {
            loader.retain(setOf(41))
            loader.prefetch(listOf(41))
            loader.prepare(41)
            assertTrue("the import never started", importStarted.await(2, TimeUnit.SECONDS))
            cancelDrainWhileWorkIsStarted(loader, releaseImport)
            assertTrue(
                "the prefetch and import answers never queued",
                answersQueued.await(2, TimeUnit.SECONDS),
            )
            while (mainHops.isNotEmpty()) mainHops.removeFirst().invoke()

            var deliveredSynchronously = false
            loader.loadSpectrogram(41) { deliveredSynchronously = it != null }

            assertTrue(deliveredSynchronously)
            assertEquals(1, reads)
            assertEquals(1L, loader.revision)
        } finally {
            while (mainHops.isNotEmpty()) mainHops.removeFirst().invoke()
        }
    }

    @Test
    fun nullAnalysisEntriesAreNotReportedAsWarm() {
        val mainHops = ArrayDeque<() -> Unit>()
        val readsFinished = CountDownLatch(2)
        val loader = TrackAnalysisLoader(
            importAnalysis = { AndroidAnalysisOutcome.IMPORTED },
            readBars = { _, _ ->
                readsFinished.countDown()
                null
            },
            readSpectrogram = {
                readsFinished.countDown()
                null
            },
            onMainThread = mainHops::add,
        )

        loader.retain(setOf(41))
        loader.loadBars(41, 64) {}
        loader.loadSpectrogram(41) {}
        assertTrue("the null reads never finished", readsFinished.await(2, TimeUnit.SECONDS))
        loader.shutdownForTest()
        while (mainHops.isNotEmpty()) mainHops.removeFirst().invoke()

        assertFalse(loader.warmth(41).bars)
        assertFalse(loader.warmth(41).spectrogram)
    }

    @Test
    fun retainingTheFiveTrackWindowEvictsAnalysisOutsideIt() {
        val mainHops = ArrayDeque<() -> Unit>()
        val firstReads = CountDownLatch(5)
        val evictedRead = CountDownLatch(1)
        val reads = mutableMapOf<Long, Int>()
        val loader = TrackAnalysisLoader(
            importAnalysis = { AndroidAnalysisOutcome.IMPORTED },
            readBars = { _, _ -> null },
            readSpectrogram = { trackId ->
                reads[trackId] = reads.getOrDefault(trackId, 0) + 1
                if (reads[trackId] == 1) firstReads.countDown() else evictedRead.countDown()
                null
            },
            onMainThread = mainHops::add,
        )

        loader.prefetch(listOf(1, 2, 3, 4, 5))
        assertTrue("the initial window did not load", firstReads.await(2, TimeUnit.SECONDS))
        while (mainHops.isNotEmpty()) mainHops.removeFirst().invoke()
        loader.retain(setOf(2, 3, 4, 5, 6))

        loader.loadSpectrogram(2) {}
        loader.loadSpectrogram(1) {}

        assertTrue("the evicted track was not read again", evictedRead.await(2, TimeUnit.SECONDS))
        loader.shutdownForTest()
        assertEquals(1, reads[2])
        assertEquals(2, reads[1])
    }

    @Test
    fun firstSeekBarRequestWarmsBarsForTheRetainedWindowAtTheRealCount() {
        val mainHops = ArrayDeque<() -> Unit>()
        val readsFinished = CountDownLatch(5)
        val reads = mutableMapOf<Long, Int>()
        val loader = TrackAnalysisLoader(
            importAnalysis = { AndroidAnalysisOutcome.IMPORTED },
            readBars = { trackId, count ->
                assertEquals(64, count)
                reads[trackId] = reads.getOrDefault(trackId, 0) + 1
                readsFinished.countDown()
                emptyList()
            },
            onMainThread = mainHops::add,
        )

        loader.retain(setOf(1, 2, 3, 4, 5))
        loader.prefetch(listOf(1, 2, 3, 4, 5))
        loader.loadBars(3, 64) {}

        assertTrue("the retained bar window did not load", readsFinished.await(2, TimeUnit.SECONDS))
        loader.shutdownForTest()
        while (mainHops.isNotEmpty()) mainHops.removeFirst().invoke()

        assertEquals((1L..5L).associateWith { 1 }, reads)
        assertTrue((1L..5L).all { trackId -> loader.warmth(trackId).bars })
    }

    /**
     * Import and read run on independent lanes (a slow import must never
     * queue a bar read behind it, see [aSlowImportDoesNotBlockABarRead]);
     * this test pins the invariant that survives the split: neither runs on
     * the caller's own thread, and a worker never hands its result straight
     * to the delivery callback — it always crosses back through
     * `onMainThread`.
     */
    @Test
    fun importAndBarReadNeverRunOnTheCallerOrDeliverDirectly() {
        val caller = Thread.currentThread()
        val workerThreads = mutableListOf<Thread>()
        val mainHops = ArrayDeque<() -> Unit>()
        val imported = CountDownLatch(1)
        val read = CountDownLatch(1)
        var delivered: List<SpectralBar>? = null
        val expected = listOf(SpectralBar(false, 0.75f, 0.1, 0.2, 0.3))
        val loader = TrackAnalysisLoader(
            importAnalysis = {
                workerThreads += Thread.currentThread()
                imported.countDown()
                AndroidAnalysisOutcome.IMPORTED
            },
            readBars = { _, _ ->
                workerThreads += Thread.currentThread()
                read.countDown()
                expected
            },
            onMainThread = mainHops::add,
        )

        loader.prepare(41)
        loader.loadBars(41, 64) { delivered = it }

        assertTrue("the import never ran", imported.await(2, TimeUnit.SECONDS))
        assertTrue("the bar read never ran", read.await(2, TimeUnit.SECONDS))
        loader.shutdownForTest()
        assertTrue(workerThreads.all { it !== caller })
        assertFalse("a worker callback changed UI state directly", delivered === expected)

        while (mainHops.isNotEmpty()) mainHops.removeFirst().invoke()
        assertEquals(expected, delivered)
    }

    @Test
    fun aSlowImportDoesNotBlockABarRead() {
        val mainHops = ArrayDeque<() -> Unit>()
        val importStarted = CountDownLatch(1)
        val releaseImport = CountDownLatch(1)
        val barRead = CountDownLatch(1)
        val order = mutableListOf<String>()
        val loader = TrackAnalysisLoader(
            importAnalysis = { trackId ->
                importStarted.countDown()
                releaseImport.await()
                order += "import:$trackId"
                AndroidAnalysisOutcome.COMPUTED
            },
            readBars = { trackId, _ ->
                order += "read:$trackId"
                barRead.countDown()
                null
            },
            onMainThread = mainHops::add,
        )

        loader.prepare(41)
        assertTrue("the import never started", importStarted.await(2, TimeUnit.SECONDS))

        loader.loadBars(41, 64) {}

        assertTrue("the bar read never ran while the import was still blocked", barRead.await(2, TimeUnit.SECONDS))
        assertEquals(listOf("read:41"), order)

        releaseImport.countDown()
        loader.shutdownForTest()
        while (mainHops.isNotEmpty()) mainHops.removeFirst().invoke()
        assertEquals(listOf("read:41", "import:41"), order)
    }

    @Test
    fun aComputedAnalysisRefreshesTheBars() {
        val mainHops = ArrayDeque<() -> Unit>()
        var latch = CountDownLatch(1)
        var barsAvailable = false
        var reads = 0
        val loader = TrackAnalysisLoader(
            importAnalysis = { AndroidAnalysisOutcome.COMPUTED },
            readBars = { _, _ ->
                reads += 1
                if (barsAvailable) {
                    listOf(SpectralBar(silence = false, level = 0.5f, red = 0.0, green = 0.0, blue = 0.0))
                } else {
                    null
                }
            },
            onMainThread = { work ->
                mainHops.add(work)
                latch.countDown()
            },
        )

        var firstDelivered: List<SpectralBar>? = listOf(
            SpectralBar(silence = true, level = 0f, red = 0.0, green = 0.0, blue = 0.0),
        )
        loader.loadBars(41, 64) { firstDelivered = it }
        assertTrue("the first bar answer was never queued", latch.await(2, TimeUnit.SECONDS))
        while (mainHops.isNotEmpty()) mainHops.removeFirst().invoke()
        assertEquals(null, firstDelivered)

        barsAvailable = true
        latch = CountDownLatch(1)
        loader.prepare(41)
        assertTrue("the import answer was never queued", latch.await(2, TimeUnit.SECONDS))
        while (mainHops.isNotEmpty()) mainHops.removeFirst().invoke()
        assertEquals(1L, loader.revision)

        latch = CountDownLatch(1)
        var secondDelivered: List<SpectralBar>? = null
        loader.loadBars(41, 64) { secondDelivered = it }
        assertTrue("the second bar answer was never queued", latch.await(2, TimeUnit.SECONDS))
        while (mainHops.isNotEmpty()) mainHops.removeFirst().invoke()

        assertEquals(1, secondDelivered?.size)
        assertEquals(2, reads)

        loader.shutdownForTest()
    }
}

private fun cancelDrainWhileWorkIsStarted(
    loader: TrackAnalysisLoader,
    releaseWork: CountDownLatch,
) {
    val shutdownResult = AtomicReference<Boolean>()
    val shutdownReturned = CountDownLatch(1)
    val shutdownThread = Thread {
        shutdownResult.set(loader.shutdown())
        shutdownReturned.countDown()
    }.also(Thread::start)
    try {
        assertTrue("shutdown never entered its drain", shutdownThread.awaitWaiting())

        shutdownThread.interrupt()

        assertTrue("interrupted shutdown did not return", shutdownReturned.await(2, TimeUnit.SECONDS))
        assertFalse(shutdownResult.get())
    } finally {
        releaseWork.countDown()
    }
}

private fun Thread.awaitWaiting(): Boolean {
    val deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(2)
    while (state != Thread.State.WAITING &&
        state != Thread.State.TIMED_WAITING &&
        System.nanoTime() < deadline
    ) {
        Thread.yield()
    }
    return state == Thread.State.WAITING || state == Thread.State.TIMED_WAITING
}
