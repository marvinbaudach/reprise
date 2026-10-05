package io.github.marvinbaudach.reprise

import androidx.activity.ComponentActivity
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import io.github.marvinbaudach.reprise.ui.theme.RepriseTheme
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.reprise_android_ffi.AndroidColorScheme
import uniffi.reprise_android_ffi.AndroidReplayGainMode

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36], qualifiers = "w500dp-h1000dp")
class ReplayGainSettingsTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private val nocturneForTests = MobileThemeSelection(
        palette = MobileTheme.NOCTURNE,
        colorScheme = AndroidColorScheme.SYSTEM,
        dynamicAvailable = false,
    )

    @Test
    fun play_20_the_playback_settings_offer_off_per_track_and_per_album() {
        val chosen = mutableListOf<AndroidReplayGainMode>()
        compose.setContent {
            RepriseTheme(nocturneForTests, darkPalette = true) {
                PlaybackSettingsScreen(
                    state = PlaybackSettingsUiState(
                        equalizerEnabled = false,
                        gaplessEnabled = true,
                        equalizerBands = emptyList(),
                        replayGainMode = AndroidReplayGainMode.TRACK,
                    ),
                    themeSelection = nocturneForTests,
                    close = {},
                    setEqualizerEnabled = {},
                    replaceEqualizerCurve = {},
                    setGaplessEnabled = {},
                    selectTheme = {},
                    setReplayGainMode = { chosen += it },
                )
            }
        }

        compose.onNodeWithText("Volume Normalization").assertIsDisplayed()
        compose.onNodeWithText("Per Track").assertIsDisplayed()
        compose.onNodeWithText("Per Track").performClick()
        compose.onNodeWithText("Off").performClick()
        compose.onNodeWithText("Per Track").performClick()
        compose.onNodeWithText("Per Album").performClick()

        assertEquals(listOf(AndroidReplayGainMode.OFF, AndroidReplayGainMode.ALBUM), chosen)
    }
}
