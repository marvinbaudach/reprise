package io.github.marvinbaudach.reprise

import androidx.activity.ComponentActivity
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import io.github.marvinbaudach.reprise.ui.theme.RepriseTheme
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.reprise_android_ffi.AndroidColorScheme

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36], qualifiers = "w412dp-h916dp-port")
class MiniPlayerLayoutTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    fun theCoverIsCentredVerticallyInsideTheWholeMiniPlayerCard() {
        compose.setContent {
            RepriseTheme(theme, darkPalette = true) {
                LibraryBottomFrame(
                    surfaceLayout = SurfaceLayout.STACKED,
                    currentTrack = track,
                    playback = LibraryPlayback(),
                    progress = { 0f },
                    shownTab = { BrowseTab.TITLES },
                    selectTab = {},
                    openNowPlaying = {},
                )
            }
        }

        val player = compose.onNodeWithTag("library-mini-player").getUnclippedBoundsInRoot()
        val cover = compose.onNodeWithTag(
            "library-mini-player-cover",
            useUnmergedTree = true,
        ).getUnclippedBoundsInRoot()

        assertEquals(8f, cover.top.value - player.top.value, 0.5f)
        assertEquals(8f, player.bottom.value - cover.bottom.value, 0.5f)
    }

    @Test
    fun theTitleAndArtistAreCentredHorizontallyInTheTextColumn() {
        compose.setContent {
            RepriseTheme(theme, darkPalette = true) {
                LibraryBottomFrame(
                    surfaceLayout = SurfaceLayout.STACKED,
                    currentTrack = track,
                    playback = LibraryPlayback(),
                    progress = { 0f },
                    shownTab = { BrowseTab.TITLES },
                    selectTab = {},
                    openNowPlaying = {},
                )
            }
        }

        val column = compose.onNodeWithTag(
            "library-mini-player-text",
            useUnmergedTree = true,
        ).getUnclippedBoundsInRoot()
        val columnCentre = (column.left.value + column.right.value) / 2f
        listOf(track.title, track.artist).forEach { text ->
            val bounds = compose.onNodeWithText(text, useUnmergedTree = true)
                .getUnclippedBoundsInRoot()
            assertEquals(columnCentre, (bounds.left.value + bounds.right.value) / 2f, 0.5f)
        }
    }

    private companion object {
        val theme = MobileThemeSelection(
            palette = MobileTheme.NOCTURNE,
            colorScheme = AndroidColorScheme.SYSTEM,
            dynamicAvailable = false,
        )
        val track = LibraryTrack(
            id = 1,
            uri = "content://provider/document/centred.flac",
            title = "Centred",
            artist = "Artist",
            album = "Album",
            durationMs = 58_000,
            playCount = 0,
            rating = 0,
        )
    }
}
