package de.reprise.spike

import androidx.activity.ComponentActivity
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import de.reprise.spike.settings.SettingsNavigation
import de.reprise.spike.ui.theme.RepriseTheme
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.reprise_android_ffi.AndroidColorScheme

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36], qualifiers = "w500dp-h1000dp")
class SettingsContentTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    fun equalizerFadersAreReachableOnTheirOwnAudioRoute() {
        showSettings()

        compose.onNodeWithContentDescription("125 Hz equalizer band").assertDoesNotExist()
        compose.onNodeWithContentDescription("Open Audio").performClick()

        compose.onNodeWithText("Gapless playback").assertIsDisplayed()
        compose.onNodeWithContentDescription("125 Hz equalizer band").assertIsDisplayed()
        compose.onNodeWithContentDescription("1 kHz equalizer band").assertIsDisplayed()
    }

    @Test
    fun appearanceKeepsTheStoredThemeAndMaterialYouChoiceTogether() {
        showSettings(dynamicAvailable = true)

        compose.onNodeWithContentDescription("Open Appearance").performClick()

        compose.onNodeWithText("Nocturne").assertIsDisplayed()
        compose.onNodeWithText("Dynamic colour").assertIsDisplayed()
        compose.onNodeWithText("Colours from this device").assertIsDisplayed()
    }

    @Test
    fun aboutShowsOnlyBuildDerivedVersionsAndLicences() {
        showSettings()

        compose.onNodeWithContentDescription("Open About Reprise").performClick()

        compose.onNodeWithText("${BuildConfig.VERSION_NAME} (${BuildConfig.VERSION_CODE})")
            .assertIsDisplayed()
        compose.onNodeWithText(BuildConfig.REPRISE_CORE_VERSION).assertIsDisplayed()
        compose.onNodeWithText(BuildConfig.REPRISE_MOBILE_LICENSE).assertIsDisplayed()
        compose.onNodeWithText(BuildConfig.REPRISE_CORE_LICENSE).assertIsDisplayed()
    }

    @Test
    fun theLibraryPageNamesTheFolderItScans() {
        showSettings(folderName = "Music/Live")

        compose.onNodeWithContentDescription("Open Library & scan folder").performClick()

        compose.onNodeWithText("Music/Live").assertIsDisplayed()
    }

    /** An unnameable folder falls back to the count rather than to a token. */
    @Test
    fun theLibraryPageFallsBackToTheCountWhenTheFolderCannotBeNamed() {
        showSettings(folderName = null)

        compose.onNodeWithContentDescription("Open Library & scan folder").performClick()

        compose.onNodeWithText("1 folder").assertIsDisplayed()
    }

    /**
     * Both library actions hand back a catalogue, and the screen that reports
     * on the scan replaces the one this overlay is drawn inside — so the
     * overlay comes down either way. It has to come down on purpose: an
     * overlay that dissolves mid-scan without anyone deciding it is the same
     * movement with nobody behind it.
     */
    @Test
    fun theLibraryActionsLeaveSettingsBeforeTheyStart() {
        val closes = mutableListOf<String>()
        showSettings(onClose = { closes += "close" }, onRescan = { closes += "rescan" })

        compose.onNodeWithContentDescription("Open Library & scan folder").performClick()
        compose.onNodeWithContentDescription("Rescan library").performClick()

        assertEquals(listOf("close", "rescan"), closes)
    }

    private fun showSettings(
        dynamicAvailable: Boolean = false,
        folderName: String? = "Music",
        onClose: () -> Unit = {},
        onRescan: () -> Unit = {},
    ) {
        val theme = MobileThemeSelection(
            palette = MobileTheme.NOCTURNE,
            colorScheme = AndroidColorScheme.SYSTEM,
            dynamicAvailable = dynamicAvailable,
        )
        compose.setContent {
            RepriseTheme(theme, darkPalette = true) {
                SettingsNavigation(
                    state = PlaybackSettingsUiState(
                        equalizerEnabled = true,
                        gaplessEnabled = true,
                        equalizerBands = listOf(
                            EqualizerBandUi(125.0, -2.0, -12.0, 12.0),
                            EqualizerBandUi(1_000.0, 1.0, -12.0, 12.0),
                        ),
                    ),
                    titleCount = 1_824,
                    albumCount = 143,
                    artistCount = 92,
                    folderName = folderName,
                    themeSelection = theme,
                    close = onClose,
                    chooseFolder = {},
                    rescan = onRescan,
                    setEqualizerEnabled = {},
                    replaceEqualizerCurve = {},
                    setGaplessEnabled = {},
                    selectTheme = {},
                )
            }
        }
    }
}
