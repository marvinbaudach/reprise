package io.github.marvinbaudach.reprise

import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.reprise_android_ffi.AndroidArtworkSize

class LibrarySessionArtworkTest {
    @Test
    fun forgetting_artwork_misses_keeps_real_paths_and_retries_only_misses() {
        val missingReads = AtomicInteger()
        val realReads = AtomicInteger()
        var missingPath: String? = null
        val session = LibrarySession(fakeLibrarySessionPort(artworkFor = { trackUri, _ ->
            when (trackUri) {
                MISSING_URI -> {
                    missingReads.incrementAndGet()
                    missingPath
                }
                REAL_URI -> {
                    realReads.incrementAndGet()
                    REAL_PATH
                }
                else -> error("Unexpected track URI: $trackUri")
            }
        }))

        assertNull(session.artworkFor(MISSING_URI))
        assertEquals(REAL_PATH, session.artworkFor(REAL_URI))

        session.forgetArtworkMisses()
        missingPath = NEW_PATH

        assertEquals(NEW_PATH, session.artworkFor(MISSING_URI))
        assertEquals(REAL_PATH, session.artworkFor(REAL_URI))
        assertEquals(2, missingReads.get())
        assertEquals(1, realReads.get())
    }

    @Test
    fun forgetting_artwork_misses_rejects_an_in_flight_stale_miss() {
        val firstReadStarted = CountDownLatch(1)
        val releaseFirstRead = CountDownLatch(1)
        val reads = AtomicInteger()
        var path: String? = null
        val session = LibrarySession(fakeLibrarySessionPort(artworkFor = { _, _ ->
            reads.incrementAndGet()
            val answer = path
            firstReadStarted.countDown()
            assertTrue(releaseFirstRead.await(5, TimeUnit.SECONDS))
            answer
        }))
        val worker = Executors.newSingleThreadExecutor()

        try {
            val staleRead = worker.submit<String?> { session.artworkFor(MISSING_URI) }
            assertTrue(firstReadStarted.await(5, TimeUnit.SECONDS))

            session.forgetArtworkMisses()
            path = NEW_PATH
            releaseFirstRead.countDown()

            assertNull(staleRead.get(5, TimeUnit.SECONDS))
            assertEquals(NEW_PATH, session.artworkFor(MISSING_URI))
            assertEquals(2, reads.get())
        } finally {
            releaseFirstRead.countDown()
            worker.shutdownNow()
        }
    }

    private companion object {
        const val MISSING_URI = "content://tracks/missing"
        const val REAL_URI = "content://tracks/real"
        const val REAL_PATH = "/covers/real.jpg"
        const val NEW_PATH = "/covers/new.jpg"
    }
}
