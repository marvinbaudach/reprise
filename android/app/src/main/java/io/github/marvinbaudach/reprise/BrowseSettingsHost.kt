package io.github.marvinbaudach.reprise

import androidx.activity.compose.BackHandler
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import io.github.marvinbaudach.reprise.settings.SettingsNavigation
import io.github.marvinbaudach.reprise.settings.SettingsOverlay

/** What the settings overlay shows, and the one place a read or write of it can fail. */
@Stable
internal class PlaybackSettingsHost {
    var state by mutableStateOf<PlaybackSettingsUiState?>(null)
        private set

    /**
     * Replaces the settings with what [read] answers. [verb] names the attempt in
     * the message a failure leaves behind ("Could not load playback settings").
     */
    fun replace(verb: String, read: () -> PlaybackSettingsUiState) {
        state = runCatching(read).getOrElse { error ->
            failed("Could not $verb playback settings: ${error.message ?: "unknown error"}")
        }
    }

    // A failure has to leave a *state* behind, never null: null renders
    // nothing at all, and there is no previous state to fall back on the first
    // time round — or after a rotation, which throws this one away and restores
    // `settingsVisible` without it.
    private fun failed(message: String): PlaybackSettingsUiState = state?.copy(error = message)
        ?: PlaybackSettingsUiState(
            equalizerEnabled = false,
            gaplessEnabled = false,
            equalizerBands = emptyList(),
            error = message,
        )
}

@Composable
internal fun BrowseSettingsOverlay(
    visible: Boolean,
    settings: PlaybackSettingsHost,
    state: LibraryScreenState.Browse,
    surfaceState: MobileSurfaceViewModel,
    themeSelection: MobileThemeSelection,
    selectTheme: (MobileTheme) -> Unit,
    chooseFolder: () -> Unit,
    rescan: () -> Unit,
    updateSettings: (() -> PlaybackSettingsUiState) -> Unit,
    setEqualizerEnabled: (Boolean) -> PlaybackSettingsUiState,
    replaceEqualizerCurve: (List<EqualizerCurvePoint>) -> PlaybackSettingsUiState,
    setGaplessEnabled: (Boolean) -> PlaybackSettingsUiState,
    setVolumeKeySkipGestureEnabled: (Boolean) -> PlaybackSettingsUiState,
) {
    SettingsOverlay(visible = visible) {
        // Never an empty branch: a full-screen surface with no header
        // and no way back is what a rotation used to leave behind while
        // the settings were being read again.
        // Read here, inside the slot, so a change to the settings invalidates
        // the overlay and not the whole library screen.
        when (val current = settings.state) {
            null -> {
                BackHandler(enabled = visible) {
                    surfaceState.showSettings(false)
                }
                PlaybackSettingsLoading(close = { surfaceState.showSettings(false) })
            }
            else -> SettingsNavigation(
                state = current,
                titleCount = state.titles.total,
                albumCount = state.albumCount,
                artistCount = state.artists.total,
                folderName = folderLabel(state.folderUri),
                themeSelection = themeSelection,
                active = visible,
                close = { surfaceState.showSettings(false) },
                chooseFolder = chooseFolder,
                rescan = rescan,
                setEqualizerEnabled = { enabled ->
                    updateSettings { setEqualizerEnabled(enabled) }
                },
                replaceEqualizerCurve = { points ->
                    updateSettings { replaceEqualizerCurve(points) }
                },
                setGaplessEnabled = { enabled ->
                    updateSettings { setGaplessEnabled(enabled) }
                },
                setVolumeKeySkipGestureEnabled = { enabled ->
                    updateSettings { setVolumeKeySkipGestureEnabled(enabled) }
                },
                selectTheme = selectTheme,
            )
        }
    }
}
