package io.github.marvinbaudach.reprise

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class TrackAnalysisBackfillPolicyTest {
    @Test
    fun playingWithoutPowerSaveModeRuns() {
        assertTrue(analysisBackfillShouldRun(playing = true, powerSaveMode = false))
    }

    @Test
    fun playingWithPowerSaveModeDoesNotRun() {
        assertFalse(analysisBackfillShouldRun(playing = true, powerSaveMode = true))
    }

    @Test
    fun notPlayingWithoutPowerSaveModeDoesNotRun() {
        assertFalse(analysisBackfillShouldRun(playing = false, powerSaveMode = false))
    }

    @Test
    fun notPlayingWithPowerSaveModeDoesNotRun() {
        assertFalse(analysisBackfillShouldRun(playing = false, powerSaveMode = true))
    }
}
