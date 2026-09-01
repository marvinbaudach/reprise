package io.github.marvinbaudach.reprise

import androidx.activity.ComponentActivity
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsOff
import androidx.compose.ui.test.assertIsOn
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isToggleable
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import io.github.marvinbaudach.reprise.settings.OnlineSourcesSettingsPage
import io.github.marvinbaudach.reprise.ui.theme.RepriseTheme
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.reprise_android_ffi.AndroidColorScheme

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36], qualifiers = "w500dp-h1000dp")
class OnlineSourcesSettingsPageTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    fun theSwitchStartsFromTheSuppliedState() {
        showPage(enabled = true)

        compose.onNodeWithTag("settings-page-online-sources").assertIsDisplayed()
        switchNode().assertIsOn()
    }

    @Test
    fun togglingTheSwitchReportsTheNewValueOnce() {
        val changes = mutableListOf<Boolean>()
        showPage(enabled = false, setEnabled = changes::add)

        switchNode().assertIsOff().performClick()

        assertEquals(listOf(true), changes)
    }

    @Test
    fun thePageExplainsLibraryWidePortraitPrefetch() {
        showPage(enabled = false)

        compose.onNodeWithText("Fetch portraits after automatic scans, manual scans, or restores.")
            .assertIsDisplayed()
        compose.onNodeWithText(
            "Artist names in your library are sent to Deezer after an automatic scan, " +
                "manual scan, or restore.",
            substring = true,
        )
            .assertIsDisplayed()
    }

    private fun switchNode() = compose.onNode(
        hasText("Download artist photos") and isToggleable(),
    )

    private fun showPage(
        enabled: Boolean,
        setEnabled: (Boolean) -> Unit = {},
    ) {
        compose.setContent {
            RepriseTheme(theme, darkPalette = true) {
                OnlineSourcesSettingsPage(
                    enabled = enabled,
                    setEnabled = setEnabled,
                    back = {},
                )
            }
        }
    }

    private val theme = MobileThemeSelection(
        palette = MobileTheme.NOCTURNE,
        colorScheme = AndroidColorScheme.SYSTEM,
        dynamicAvailable = false,
    )
}
