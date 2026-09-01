package io.github.marvinbaudach.reprise

import androidx.activity.ComponentActivity
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
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

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36], qualifiers = "w500dp-h1000dp")
class SettingsMenuTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    fun folderChoiceLivesOnlyOnTheLibraryPageWhileBothRescanEntriesShareOneAction() {
        var rescans = 0
        var folderChoices = 0
        val browse = LibraryScreenState.Browse(
            titles = LibraryWindow(total = 1_824, rows = emptyList(), hasMore = true),
            artists = LibraryWindow(total = 92, rows = emptyList(), hasMore = true),
            albumCount = 143,
        )
        compose.setContent {
            RepriseTheme(nocturneForTests, darkPalette = true) {
                BrowseScreen(
                    state = browse,
                    playback = PlaybackUiState().libraryPlayback(),
                    playbackSettingsRevision = 0,
                    chooseFolder = { folderChoices += 1 },
                    rescan = { rescans += 1 },
                    themeSelection = nocturneForTests,
                    selectTheme = {},
                    searchTitles = { _, _ -> browse.titles },
                    listArtists = { browse.artists },
                    openAlbum = { error("Album navigation is outside this test") },
                    listAlbumTracks = { _, _ -> LibraryWindow.empty() },
                    loadTrack = { _, deliver -> deliver(null) },
                    playTracks = { _, _ -> },
                    loadPlaybackSettings = {
                        PlaybackSettingsUiState(false, true, emptyList())
                    },
                    setEqualizerEnabled = { enabled ->
                        PlaybackSettingsUiState(enabled, true, emptyList())
                    },
                    replaceEqualizerCurve = {
                        PlaybackSettingsUiState(false, true, emptyList())
                    },
                    setGaplessEnabled = { enabled ->
                        PlaybackSettingsUiState(false, enabled, emptyList())
                    },
                )
            }
        }

        openMenu()
        compose.onNodeWithText("Choose another folder").assertDoesNotExist()
        compose.onNodeWithText("Rescan").assertIsDisplayed().performClick()
        assertEquals(1, rescans)

        openMenu()
        compose.onNodeWithText("Settings").performClick()
        compose.onNodeWithText("Library & scan folder").performClick()
        compose.onNodeWithText("Choose another folder").assertIsDisplayed().performClick()
        // Leaving settings is part of both actions: what reports on the scan is
        // a screen that replaces the one this overlay is drawn inside, so the
        // overlay goes either way — deliberately, rather than dissolving under
        // the listener halfway through.
        compose.onNodeWithText("Choose another folder").assertDoesNotExist()
        assertEquals(1, folderChoices)

        openMenu()
        compose.onNodeWithText("Settings").performClick()
        compose.onNodeWithText("Library & scan folder").performClick()
        compose.onNodeWithContentDescription("Rescan library").performClick()
        compose.onNodeWithContentDescription("Rescan library").assertDoesNotExist()

        assertEquals(2, rescans)
    }

    private fun openMenu() {
        compose.onNodeWithContentDescription("Library actions").performClick()
    }

    private companion object {
        val nocturneForTests = MobileThemeSelection(
            palette = MobileTheme.NOCTURNE,
            colorScheme = AndroidColorScheme.SYSTEM,
            dynamicAvailable = false,
        )
    }
}
