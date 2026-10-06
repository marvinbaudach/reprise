package io.github.marvinbaudach.reprise

import org.junit.Assert.assertEquals
import org.junit.Test

class ArtistPhotoProgressSummaryTest {
    @Test
    fun everyPhaseMapsToItsSummarySuffixWithAndWithoutFailures() {
        val cases = listOf(
            progress(ArtistPhotoProgressPhase.PREPARING) to " · Preparing artwork",
            progress(ArtistPhotoProgressPhase.PREPARING, failed = 2) to " · Preparing artwork",
            progress(ArtistPhotoProgressPhase.RUNNING) to " · Artwork 3/8",
            progress(ArtistPhotoProgressPhase.RUNNING, failed = 2) to " · Artwork 3/8",
            progress(ArtistPhotoProgressPhase.PAUSED) to " · Waiting for a connection",
            progress(ArtistPhotoProgressPhase.PAUSED, failed = 2) to " · Waiting for a connection",
            progress(ArtistPhotoProgressPhase.COMPLETE) to "",
            progress(ArtistPhotoProgressPhase.COMPLETE, failed = 2) to " · 2 without a photo",
        )

        cases.forEach { (progress, expected) ->
            assertEquals(expected, artistPhotoProgressSummarySuffix(progress))
        }
        assertEquals("", artistPhotoProgressSummarySuffix(null))
    }

    private fun progress(
        phase: ArtistPhotoProgressPhase,
        failed: Long = 0,
    ) = ArtistPhotoProgress(
        runId = 7,
        phase = phase,
        done = 3,
        failed = failed,
        total = 8,
    )
}
