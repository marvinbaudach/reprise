package de.reprise.spike

import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.reprise_android_ffi.AndroidTrackRenderBar

class TrackAnalysisBoundaryTest {
    @Test
    fun rustBoundaryChannelsReachTheDrawableBarWithoutTransposition() {
        val rustBar = AndroidTrackRenderBar(
            silence = false,
            level = 0.75f,
            red = 0.125,
            green = 0.5,
            blue = 0.875,
        )

        assertEquals(
            SpectralBar(
                silence = false,
                level = 0.75f,
                red = 0.125,
                green = 0.5,
                blue = 0.875,
            ),
            rustBar.toSpectralBar(),
        )
    }
}
