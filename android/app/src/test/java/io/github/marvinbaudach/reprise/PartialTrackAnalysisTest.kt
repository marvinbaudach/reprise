package io.github.marvinbaudach.reprise

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import uniffi.reprise_android_ffi.AndroidTrackAnalysisProgress
import uniffi.reprise_android_ffi.AndroidTrackSpectrogram

/** The progress record from Rust is checked where it crosses into Kotlin. */
class PartialTrackAnalysisTest {
    @Test
    fun nav_15d_a_covered_fraction_outside_the_track_is_clamped() {
        assertEquals(1f, progress(1.5f).toPartialTrackAnalysis()?.coveredFraction)
        assertEquals(0.25f, progress(0.25f).toPartialTrackAnalysis()?.coveredFraction)
    }

    @Test
    fun nav_15d_an_empty_or_broken_covered_fraction_is_no_partial() {
        assertNull(progress(0f).toPartialTrackAnalysis())
        assertNull(progress(-0.5f).toPartialTrackAnalysis())
        assertNull(progress(Float.NaN).toPartialTrackAnalysis())
        assertNull(progress(Float.POSITIVE_INFINITY).toPartialTrackAnalysis())
    }

    private fun progress(fraction: Float) = AndroidTrackAnalysisProgress(
        coveredFraction = fraction,
        bars = emptyList(),
        spectrogram = AndroidTrackSpectrogram(bandCount = 24u, frameRateHz = 20u, cells = ByteArray(0)),
    )
}
