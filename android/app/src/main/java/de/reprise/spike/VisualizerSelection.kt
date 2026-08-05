package de.reprise.spike

import androidx.compose.runtime.staticCompositionLocalOf
import uniffi.reprise_android_ffi.AndroidStoredVisualizer
import uniffi.reprise_android_ffi.AndroidVisualizerChoice
import uniffi.reprise_android_ffi.MusicLibrary

internal enum class MobileVisualizer(
    val label: String,
    private val requiresTrackAnalysis: Boolean,
) {
    // The plan that specified these was written in German for its reader and
    // named the modes in German prose. They are identifiers there, not strings
    // to ship: every other label in this app is English.
    COVER("Cover", false),
    SPECTRUM("Spectrum", true),
    PREVIEW_BAND("Preview", true),
    AMBIENT("Ambient", false),
    ;

    fun isAvailable(trackAnalysed: Boolean): Boolean =
        !requiresTrackAnalysis || trackAnalysed
}

internal interface VisualizerSettingsPort {
    fun visualizerSetting(): AndroidStoredVisualizer

    fun setVisualizer(visualizer: AndroidVisualizerChoice)
}

internal class AndroidVisualizerSettingsPort(
    private val library: MusicLibrary,
) : VisualizerSettingsPort {
    override fun visualizerSetting(): AndroidStoredVisualizer = library.visualizerSetting()

    override fun setVisualizer(visualizer: AndroidVisualizerChoice) {
        library.setVisualizer(visualizer)
    }
}

/** Resolves the shared setting without turning a fallback into a write. */
internal class VisualizerController(
    private val port: VisualizerSettingsPort,
) {
    fun load(): MobileVisualizer = when (port.visualizerSetting()) {
        AndroidStoredVisualizer.Ambient -> MobileVisualizer.AMBIENT
        AndroidStoredVisualizer.Cover -> MobileVisualizer.COVER
        AndroidStoredVisualizer.PreviewBand -> MobileVisualizer.PREVIEW_BAND
        AndroidStoredVisualizer.Spectrum -> MobileVisualizer.SPECTRUM
        AndroidStoredVisualizer.Unset,
        is AndroidStoredVisualizer.Unsupported,
        -> MobileVisualizer.COVER
    }

    fun select(visualizer: MobileVisualizer): MobileVisualizer {
        port.setVisualizer(
            when (visualizer) {
                MobileVisualizer.COVER -> AndroidVisualizerChoice.COVER
                MobileVisualizer.SPECTRUM -> AndroidVisualizerChoice.SPECTRUM
                MobileVisualizer.PREVIEW_BAND -> AndroidVisualizerChoice.PREVIEW_BAND
                MobileVisualizer.AMBIENT -> AndroidVisualizerChoice.AMBIENT
            },
        )
        return visualizer
    }
}

internal data class VisualizerControl(
    val selected: MobileVisualizer,
    val select: (MobileVisualizer) -> Unit,
)

internal val LocalVisualizerControl = staticCompositionLocalOf {
    VisualizerControl(MobileVisualizer.COVER) {}
}
