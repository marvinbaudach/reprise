package de.reprise.spike

import androidx.compose.ui.FrameRateCategory
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class NowPlayingFrameRateTest {
    @Test
    fun onlyAVisiblePlayingVisualizerRequestsTheHighFrameRateCategory() {
        assertTrue(shouldRequestHighVisualizerFrameRate(visualizerOpacity = 1f, playing = true))
        assertTrue(shouldRequestHighVisualizerFrameRate(visualizerOpacity = 0.01f, playing = true))

        assertFalse(shouldRequestHighVisualizerFrameRate(visualizerOpacity = 0f, playing = true))
        assertFalse(shouldRequestHighVisualizerFrameRate(visualizerOpacity = 1f, playing = false))
        assertFalse(shouldRequestHighVisualizerFrameRate(Float.NaN, playing = true))
        assertEquals(
            FrameRateCategory.High,
            requestedVisualizerFrameRateCategory(visualizerOpacity = 1f, playing = true),
        )
        assertNull(requestedVisualizerFrameRateCategory(visualizerOpacity = 1f, playing = false))
    }
}
