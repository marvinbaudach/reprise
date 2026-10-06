package io.github.marvinbaudach.reprise

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.reprise_android_ffi.AndroidAnalysisOutcome

class TrackAnalysisRetryTest {
    @Test
    fun nav_15c_only_cancelled_changed_or_thrown_analysis_is_non_final() {
        assertTrue(trackAnalysisIsNonFinal(AndroidAnalysisOutcome.CANCELLED))
        assertTrue(trackAnalysisIsNonFinal(AndroidAnalysisOutcome.PHONE_SOURCE_CHANGED))
        assertTrue(trackAnalysisIsNonFinal(null, IllegalStateException("decode stopped")))

        assertFalse(trackAnalysisIsNonFinal(AndroidAnalysisOutcome.COMPUTED))
        assertFalse(trackAnalysisIsNonFinal(AndroidAnalysisOutcome.DECODE_FAILED))
        assertFalse(trackAnalysisIsNonFinal(null))
        assertTrue(MAX_ANALYSIS_ATTEMPTS == 3)
    }

    @Test
    fun nav_15e_superseded_is_final() {
        assertFalse(trackAnalysisIsNonFinal(AndroidAnalysisOutcome.SUPERSEDED))
    }
}
