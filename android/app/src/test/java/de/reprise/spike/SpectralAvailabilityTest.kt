package de.reprise.spike

import androidx.activity.ComponentActivity
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.unit.dp
import de.reprise.spike.settings.SettingsNavigation
import de.reprise.spike.ui.theme.RepriseTheme
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.reprise_android_ffi.AndroidColorScheme

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36], qualifiers = "w412dp-h916dp-port")
class SpectralAvailabilityTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private val renderData = AvailabilityRenderData()

    @Test
    fun appearanceEntriesFollowAnalysisForThePlayingTrack() {
        showSettings()
        compose.onNodeWithContentDescription("Open Appearance").performClick()

        compose.onNodeWithTag("settings-visualizer-SPECTRUM")
            .assertIsNotEnabled()
            .assertTextEquals("Spectrum", "Needs track analysis")
        compose.onNodeWithTag("settings-visualizer-PREVIEW_BAND")
            .assertIsNotEnabled()
            .assertTextEquals("Preview", "Needs track analysis")

        renderData.analysed = true
        renderData.revision += 1
        compose.waitForIdle()

        compose.onNodeWithTag("settings-visualizer-SPECTRUM")
            .assertIsEnabled()
            .assertTextEquals("Spectrum")
        compose.onNodeWithTag("settings-visualizer-PREVIEW_BAND")
            .assertIsEnabled()
            .assertTextEquals("Preview")
    }

    @Test
    fun longPressMenuEntriesFollowTheSameTrackAnalysisState() {
        showNowPlayingVisualizer()
        compose.onNodeWithTag("visualizer-surface").performTouchInput {
            down(Offset(width * 0.7f, height * 0.7f))
            advanceEventTime(600)
            up()
        }

        compose.onNodeWithTag("visualizer-menu-SPECTRUM")
            .assertIsNotEnabled()
            .assertTextEquals("Spectrum", "Needs track analysis")
        compose.onNodeWithTag("visualizer-menu-PREVIEW_BAND")
            .assertIsNotEnabled()
            .assertTextEquals("Preview", "Needs track analysis")

        renderData.analysed = true
        renderData.revision += 1
        compose.waitForIdle()

        compose.onNodeWithTag("visualizer-menu-SPECTRUM")
            .assertIsEnabled()
            .assertTextEquals("Spectrum")
        compose.onNodeWithTag("visualizer-menu-PREVIEW_BAND")
            .assertIsEnabled()
            .assertTextEquals("Preview")
    }

    private fun showSettings() {
        compose.setContent {
            RepriseTheme(theme, darkPalette = true) {
                CompositionLocalProvider(LocalTrackRenderData provides renderData) {
                    SettingsNavigation(
                        state = PlaybackSettingsUiState(false, false, emptyList()),
                        titleCount = 1,
                        albumCount = 1,
                        artistCount = 1,
                        folderName = "Music",
                        themeSelection = theme,
                        playingTrackId = TRACK_ID,
                        close = {},
                        chooseFolder = {},
                        rescan = {},
                        setEqualizerEnabled = {},
                        replaceEqualizerCurve = {},
                        setGaplessEnabled = {},
                        selectTheme = {},
                    )
                }
            }
        }
        compose.waitForIdle()
    }

    private fun showNowPlayingVisualizer() {
        compose.setContent {
            RepriseTheme(theme, darkPalette = true) {
                CompositionLocalProvider(
                    LocalTrackRenderData provides renderData,
                    LocalVisualizerControl provides VisualizerControl(MobileVisualizer.COVER) {},
                ) {
                    NowPlayingVisualizer(
                        trackId = TRACK_ID,
                        trackUri = "content://provider/document/track.flac",
                        playbackFraction = 0.4f,
                        size = 240,
                        shape = RoundedCornerShape(16.dp),
                    )
                }
            }
        }
        compose.waitForIdle()
    }

    private class AvailabilityRenderData : TrackRenderDataPort {
        override var revision by mutableIntStateOf(0)
        var analysed = false

        override fun bars(trackId: Long, barCount: Int): List<TrackRenderBar>? =
            if (analysed) emptyList() else null

        override fun spectrumColumn(trackId: Long, position: Float): List<Int>? =
            if (analysed) emptyList() else null
    }

    private companion object {
        const val TRACK_ID = 88L

        val theme = MobileThemeSelection(
            palette = MobileTheme.NOCTURNE,
            colorScheme = AndroidColorScheme.SYSTEM,
            dynamicAvailable = false,
        )
    }
}
