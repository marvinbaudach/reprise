package de.reprise.spike

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.reprise_android_ffi.AndroidStoredLibraryRating

class LibraryRatingSettingTest {
    @Test
    fun loadReadsOnceAndTurnsOnlyStoredOnIntoTrueWithoutWritingBack() {
        val cases = listOf(
            AndroidStoredLibraryRating.Unset to false,
            AndroidStoredLibraryRating.Off to false,
            AndroidStoredLibraryRating.On to true,
            AndroidStoredLibraryRating.Unsupported("future-choice") to false,
        )

        cases.forEach { (stored, expected) ->
            val port = RecordingLibraryRatingSettingPort(stored)

            val enabled = LibraryRatingSettingController(port).load()

            assertEquals(expected, enabled)
            assertEquals("load must read the boundary exactly once", 1, port.reads)
            assertTrue("fallback reads must not author a value", port.writes.isEmpty())
        }
    }

    @Test
    fun explicitSelectionWritesThroughTheTypedPort() {
        val port = RecordingLibraryRatingSettingPort(AndroidStoredLibraryRating.Unset)
        val controller = LibraryRatingSettingController(port)

        val enabled = controller.select(true)

        assertTrue(enabled)
        assertEquals(listOf(true), port.writes)
        assertEquals("an explicit write does not need a second read", 0, port.reads)
    }
}

private class RecordingLibraryRatingSettingPort(
    private val stored: AndroidStoredLibraryRating,
) : LibraryRatingSettingPort {
    var reads = 0
        private set
    val writes = mutableListOf<Boolean>()

    override fun libraryRatingSetting(): AndroidStoredLibraryRating {
        reads += 1
        return stored
    }

    override fun setLibraryRating(enabled: Boolean) {
        writes += enabled
    }
}
