package io.github.marvinbaudach.reprise

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class PlaybackErrorSummaryTest {
    @Test
    fun errorWithoutCauseKeepsTheMedia3Summary() {
        val error = IllegalStateException("Source error")

        assertEquals(
            "ERROR_CODE_IO_UNSPECIFIED: Source error",
            playbackErrorSummary("ERROR_CODE_IO_UNSPECIFIED", error),
        )
    }

    @Test
    fun errorWithCauseNamesTheUnderlyingFailure() {
        val error = IllegalStateException(
            "Source error",
            java.io.FileNotFoundException("No such file or directory"),
        )

        assertEquals(
            "ERROR_CODE_IO_UNSPECIFIED: Source error — " +
                "FileNotFoundException: No such file or directory",
            playbackErrorSummary("ERROR_CODE_IO_UNSPECIFIED", error),
        )
    }

    @Test
    fun causeChainStopsAfterThreeLinks() {
        val error = IllegalStateException(
            "Source error",
            java.io.IOException(
                "first",
                SecurityException(
                    "second",
                    IllegalArgumentException(
                        "third",
                        UnsupportedOperationException("must not be included"),
                    ),
                ),
            ),
        )

        assertEquals(
            "ERROR_CODE_IO_UNSPECIFIED: Source error — " +
                "IOException: first — SecurityException: second — " +
                "IllegalArgumentException: third",
            playbackErrorSummary("ERROR_CODE_IO_UNSPECIFIED", error),
        )
    }

    @Test
    fun causeWithoutMessageStillNamesItsType() {
        val error = IllegalStateException("Source error", java.io.EOFException())

        assertEquals(
            "ERROR_CODE_IO_UNSPECIFIED: Source error — EOFException",
            playbackErrorSummary("ERROR_CODE_IO_UNSPECIFIED", error),
        )
    }

    @Test
    fun cyclicCauseChainNamesEachThrowableOnlyOnce() {
        val first = IllegalArgumentException("first")
        val second = UnsupportedOperationException("second")
        first.initCause(second)
        second.initCause(first)
        val error = IllegalStateException("Source error", first)

        assertEquals(
            "ERROR_CODE_IO_UNSPECIFIED: Source error — " +
                "IllegalArgumentException: first — UnsupportedOperationException: second",
            playbackErrorSummary("ERROR_CODE_IO_UNSPECIFIED", error),
        )
    }

    @Test
    fun pathologicalMessageIsTruncatedAtTheSummaryLimit() {
        val error = IllegalStateException("x".repeat(2_000))

        val summary = playbackErrorSummary("ERROR_CODE_IO_UNSPECIFIED", error)

        assertEquals(1_024, summary.length)
        assertTrue(summary.startsWith("ERROR_CODE_IO_UNSPECIFIED: "))
        assertTrue(summary.endsWith("…"))
    }

    @Test
    fun truncationDoesNotSplitASupplementaryCharacter() {
        val prefix = "ERROR_CODE_IO_UNSPECIFIED: "
        val message = "x".repeat(1_022 - prefix.length) + "🙂" + "tail"

        val summary = playbackErrorSummary(
            "ERROR_CODE_IO_UNSPECIFIED",
            IllegalStateException(message),
        )

        assertEquals(prefix + "x".repeat(1_022 - prefix.length) + "…", summary)
    }
}
