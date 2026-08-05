package de.reprise.spike

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.reprise_android_ffi.AndroidStoredVisualizer
import uniffi.reprise_android_ffi.AndroidVisualizerChoice

class VisualizerSelectionTest {
    @Test
    fun loadingFallsBackWithoutWritingAndOnlyAnExplicitChoiceAuthorsTheSharedValue() {
        val port = RecordingVisualizerSettingsPort(
            AndroidStoredVisualizer.Unsupported("future-visualizer"),
        )
        val controller = VisualizerController(port)

        val loaded = controller.load()

        assertEquals(MobileVisualizer.COVER, loaded)
        assertTrue(port.writes.isEmpty())

        assertEquals(MobileVisualizer.AMBIENT, controller.select(MobileVisualizer.AMBIENT))
        assertEquals(listOf(AndroidVisualizerChoice.AMBIENT), port.writes)
    }
}

private class RecordingVisualizerSettingsPort(
    private val stored: AndroidStoredVisualizer,
) : VisualizerSettingsPort {
    val writes = mutableListOf<AndroidVisualizerChoice>()

    override fun visualizerSetting(): AndroidStoredVisualizer = stored

    override fun setVisualizer(visualizer: AndroidVisualizerChoice) {
        writes += visualizer
    }
}
