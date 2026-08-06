package de.reprise.spike

import androidx.compose.runtime.staticCompositionLocalOf
import uniffi.reprise_android_ffi.AndroidStoredVisualizer
import uniffi.reprise_android_ffi.AndroidVisualizerChoice
import uniffi.reprise_android_ffi.MusicLibrary

/**
 * What the Now Playing surface can show.
 *
 * Spectrum and the preview band are absent rather than disabled: they need a
 * stored spectrogram, the phone no longer computes one, and nothing brings one
 * across yet. An entry that can never become available is worse than no entry —
 * "Needs track analysis" would be an explanation for a wait that never ends.
 * They return with whatever package makes the desktop's analyses reach a phone.
 */
internal enum class MobileVisualizer(val label: String) {
    // The plan that specified these was written in German for its reader and
    // named the modes in German prose. They are identifiers there, not strings
    // to ship: every other label in this app is English.
    COVER("Cover"),
    AMBIENT("Ambient"),
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
        // A stored spectrum or preview band is a choice this surface can no
        // longer render. It falls back to the cover *without writing*, the way
        // an unknown id does: the value belongs to whoever stored it, and a
        // fallback that overwrote it would decide for the other surface.
        AndroidStoredVisualizer.PreviewBand,
        AndroidStoredVisualizer.Spectrum,
        AndroidStoredVisualizer.Unset,
        is AndroidStoredVisualizer.Unsupported,
        -> MobileVisualizer.COVER
    }

    fun select(visualizer: MobileVisualizer): MobileVisualizer {
        port.setVisualizer(
            when (visualizer) {
                MobileVisualizer.COVER -> AndroidVisualizerChoice.COVER
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
