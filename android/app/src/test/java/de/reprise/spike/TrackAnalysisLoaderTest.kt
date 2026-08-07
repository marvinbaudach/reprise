package de.reprise.spike

import java.util.ArrayDeque
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class TrackAnalysisLoaderTest {
    @Test
    fun importAndBarReadShareOneOffMainThreadLaneInThatOrder() {
        val caller = Thread.currentThread()
        val operations = mutableListOf<String>()
        val workerThreads = mutableListOf<Thread>()
        val mainHops = ArrayDeque<() -> Unit>()
        val imported = CountDownLatch(1)
        var delivered: List<SpectralBar>? = null
        val expected = listOf(SpectralBar(false, 0.75f, 0.1, 0.2, 0.3))
        val loader = TrackAnalysisLoader(
            importAnalysis = { trackId ->
                operations += "import:$trackId"
                workerThreads += Thread.currentThread()
                imported.countDown()
            },
            readBars = { trackId, count ->
                operations += "read:$trackId:$count"
                workerThreads += Thread.currentThread()
                expected
            },
            onMainThread = mainHops::add,
        )

        loader.prepare(41)
        loader.loadBars(41, 64) { delivered = it }

        assertTrue("the import never ran", imported.await(2, TimeUnit.SECONDS))
        loader.shutdownForTest()
        assertEquals(listOf("import:41", "read:41:64"), operations)
        assertTrue(workerThreads.all { it !== caller })
        assertEquals(workerThreads.first(), workerThreads.last())
        assertFalse("a worker callback changed UI state directly", delivered === expected)

        while (mainHops.isNotEmpty()) mainHops.removeFirst().invoke()
        assertEquals(expected, delivered)
    }
}
