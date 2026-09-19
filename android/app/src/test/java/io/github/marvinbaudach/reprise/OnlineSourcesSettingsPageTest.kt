package io.github.marvinbaudach.reprise

import androidx.activity.ComponentActivity
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.isToggleable
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import io.github.marvinbaudach.reprise.settings.OnlineSourcesSettingsPage
import io.github.marvinbaudach.reprise.ui.theme.RepriseTheme
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
    fun thePageHasNoSwitch() {
        showPage()

        compose.onNodeWithTag("settings-page-online-sources").assertIsDisplayed()
        compose.onAllNodes(isToggleable()).assertCountEquals(0)
    }

    @Test
    fun thePageNamesTheThreeSources() {
        showPage()

        compose.onAllNodesWithText("Deezer", substring = true)[0].assertIsDisplayed()
        compose.onAllNodesWithText("MusicBrainz", substring = true)[0].assertIsDisplayed()
        compose.onAllNodesWithText("Cover Art Archive", substring = true)[0].assertIsDisplayed()
    }

    @Test
    fun thePageNamesTheCoverArtArchiveAsARecipient() {
        showPage()

        compose.onNodeWithText(
            "The Cover Art Archive then receives that release's identifier",
            substring = true,
        )
            .assertIsDisplayed()
    }

    @Test
    fun thePageNamesTheCoverFetchPolicy() {
        showPage()

        compose.onNodeWithText(
            "A cover is only fetched for an album that has none",
            substring = true,
        )
            .assertIsDisplayed()
    }

    @Test
    fun thePageShowsARunningBackfill() {
        val progress = mutableStateOf<ArtistPhotoProgress?>(null)
        compose.setContent {
            RepriseTheme(theme, darkPalette = true) {
                OnlineSourcesSettingsPage(progress = progress.value, back = {})
            }
        }
        compose.onNodeWithTag("artist-photo-progress").assertDoesNotExist()

        compose.runOnIdle {
            progress.value = ArtistPhotoProgress(
                runId = 1,
                phase = ArtistPhotoProgressPhase.RUNNING,
                done = 1,
                failed = 0,
                total = 412,
            )
        }

        compose.onNodeWithTag("artist-photo-progress").assertIsDisplayed()
    }

    private fun showPage() {
        compose.setContent {
            RepriseTheme(theme, darkPalette = true) {
                OnlineSourcesSettingsPage(back = {})
            }
        }
    }

    private val theme = MobileThemeSelection(
        palette = MobileTheme.NOCTURNE,
        colorScheme = AndroidColorScheme.SYSTEM,
        dynamicAvailable = false,
    )
}
