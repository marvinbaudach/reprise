package io.github.marvinbaudach.reprise

import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.yield
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotSame
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

class LibraryRestoreTest {
    @Test
    fun restoreStaysLoadingWhileDatabaseWorkRunsOffTheCallerThread() {
        val restoreStarted = CountDownLatch(1)
        val releaseRestore = CountDownLatch(1)
        val callerThread = Thread.currentThread()
        var restoreThread = callerThread
        var reportThread: Thread? = null
        var state: LibraryScreenState = LibraryScreenState.Scanning()
        val restored = LibraryScreenState.NoFolder("restored")

        Executors.newSingleThreadExecutor { work ->
            Thread(work, "library-restore-worker")
        }.asCoroutineDispatcher().use { worker ->
            runBlocking {
                val job = launchLibraryRestore(
                    dispatcher = worker,
                    restore = {
                        restoreThread = Thread.currentThread()
                        restoreStarted.countDown()
                        assertTrue(releaseRestore.await(5, TimeUnit.SECONDS))
                        restored
                    },
                    report = { result ->
                        reportThread = Thread.currentThread()
                        state = result
                    },
                )

                yield()
                assertTrue(restoreStarted.await(5, TimeUnit.SECONDS))
                assertTrue(state is LibraryScreenState.Scanning)
                releaseRestore.countDown()
                job.join()
            }
        }

        assertNotSame(callerThread, restoreThread)
        assertSame(callerThread, reportThread)
        assertEquals(restored, state)
    }
}
