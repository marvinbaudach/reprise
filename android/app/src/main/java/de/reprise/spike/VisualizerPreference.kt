package de.reprise.spike

import androidx.compose.runtime.staticCompositionLocalOf
import uniffi.reprise_android_ffi.AndroidStoredVisualizer
import uniffi.reprise_android_ffi.AndroidVisualizerChoice
import uniffi.reprise_android_ffi.MusicLibrary

internal interface VisualizerPreference {
    fun visualizerSetting(): AndroidStoredVisualizer

    fun setVisualizer(choice: AndroidVisualizerChoice, report: (Result<Unit>) -> Unit)
}

internal object DisconnectedVisualizerPreference : VisualizerPreference {
    override fun visualizerSetting(): AndroidStoredVisualizer = AndroidStoredVisualizer.Unset

    override fun setVisualizer(choice: AndroidVisualizerChoice, report: (Result<Unit>) -> Unit) {
        report(Result.success(Unit))
    }
}

/** Defers opening the shared library until the play view first asks for this setting. */
internal class AndroidVisualizerPreference(
    private val libraryWrites: LibraryWrites,
    private val library: () -> MusicLibrary,
) : VisualizerPreference {
    override fun visualizerSetting(): AndroidStoredVisualizer = library().visualizerSetting()

    override fun setVisualizer(choice: AndroidVisualizerChoice, report: (Result<Unit>) -> Unit) {
        libraryWrites.submitAnswered(
            work = {
                requireOffMainThread("Visualizer preference")
                library().setVisualizer(choice)
            },
            report = report,
        )
    }
}

internal val LocalVisualizerPreference = staticCompositionLocalOf<VisualizerPreference> {
    DisconnectedVisualizerPreference
}

internal fun AndroidStoredVisualizer.showsSpectrum(): Boolean =
    this == AndroidStoredVisualizer.Spectrum
