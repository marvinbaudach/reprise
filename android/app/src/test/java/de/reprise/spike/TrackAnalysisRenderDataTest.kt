package de.reprise.spike

import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class TrackAnalysisRenderDataTest {
    @Test
    fun shapedBarsCrossTheLibraryBoundaryOffThreadAndPublishThroughRevision() {
        val callingThread = Thread.currentThread().threadId()
        var readThread = callingThread
        val mainAnswers = ArrayDeque<() -> Unit>()
        val readFinished = CountDownLatch(1)
        val answerQueued = CountDownLatch(1)
        val expected = listOf(TrackRenderBar(false, 0.75f, 1f, 0.25f, 0.5f))
        val data = LibraryTrackRenderData(
            readBars = { _, _ ->
                readThread = Thread.currentThread().threadId()
                readFinished.countDown()
                expected
            },
            readSpectrum = { _, _ -> null },
            onMainThread = { work ->
                mainAnswers.addLast(work)
                answerQueued.countDown()
            },
        )

        assertNull(data.bars(trackId = 7, barCount = 40))
        assertTrue(readFinished.await(2, TimeUnit.SECONDS))
        assertTrue(answerQueued.await(2, TimeUnit.SECONDS))
        assertNotEquals(callingThread, readThread)
        assertEquals(0, data.revision)

        mainAnswers.removeFirst().invoke()

        assertEquals(expected, data.bars(trackId = 7, barCount = 40))
        assertEquals(1, data.revision)
        data.shutdown()
    }

    @Test
    fun spectrumBytesBecomeUnsignedOnlyAfterTheBackgroundReadAnswers() {
        val callingThread = Thread.currentThread().threadId()
        var readThread = callingThread
        val mainAnswers = ArrayDeque<() -> Unit>()
        val readFinished = CountDownLatch(1)
        val answerQueued = CountDownLatch(1)
        val data = LibraryTrackRenderData(
            readBars = { _, _ -> null },
            readSpectrum = { _, _ ->
                readThread = Thread.currentThread().threadId()
                readFinished.countDown()
                byteArrayOf(0, 127, -1)
            },
            onMainThread = { work ->
                mainAnswers.addLast(work)
                answerQueued.countDown()
            },
        )

        assertNull(data.spectrumColumn(trackId = 9, position = 0.5f))
        assertTrue(readFinished.await(2, TimeUnit.SECONDS))
        assertTrue(answerQueued.await(2, TimeUnit.SECONDS))
        assertNotEquals(callingThread, readThread)
        mainAnswers.removeFirst().invoke()

        assertEquals(listOf(0, 127, 255), data.spectrumColumn(9, 0.5f))
        data.shutdown()
    }

    @Test
    fun anAtomicStoreBumpsTheRevisionWithoutClaimingSourceChanged() {
        val data = LibraryTrackRenderData(
            readBars = { _, _ -> null },
            readSpectrum = { _, _ -> null },
            onMainThread = { it() },
        )

        data.analysisStored(7)

        assertEquals(1, data.revision)
        data.shutdown()
    }

    @Test
    fun anOlderAbsentReadCannotHideDataThatLandedWhileItWasInFlight() {
        val mainAnswers = ArrayDeque<() -> Unit>()
        val readsFinished = CountDownLatch(2)
        val answersQueued = CountDownLatch(2)
        var read = 0
        val expected = listOf(TrackRenderBar(false, 1f, 1f, 0f, 0f))
        val data = LibraryTrackRenderData(
            readBars = { _, _ ->
                read += 1
                readsFinished.countDown()
                if (read == 1) null else expected
            },
            readSpectrum = { _, _ -> null },
            onMainThread = { work ->
                mainAnswers.addLast(work)
                answersQueued.countDown()
            },
        )

        assertNull(data.bars(7, 40))
        data.analysisStored(7)
        assertNull(data.bars(7, 40))
        assertTrue(readsFinished.await(2, TimeUnit.SECONDS))
        assertTrue(answersQueued.await(2, TimeUnit.SECONDS))
        while (mainAnswers.isNotEmpty()) mainAnswers.removeFirst().invoke()

        assertEquals(expected, data.bars(7, 40))
        data.shutdown()
    }
}
