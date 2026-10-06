package io.github.marvinbaudach.reprise

import android.content.res.Configuration
import androidx.activity.ComponentActivity
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Scaffold
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.DpRect
import androidx.compose.ui.unit.dp
import io.github.marvinbaudach.reprise.ui.theme.NocturneTypography
import io.github.marvinbaudach.reprise.ui.theme.RepriseTheme
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RuntimeEnvironment
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.reprise_android_ffi.AndroidColorScheme

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36], qualifiers = "w500dp-h1000dp")
class MiniPlayerFontScaleTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    fun miniPlayerDoesNotClipTextAtDoubleFontScale() {
        showMiniPlayer(fontScale = 2f)

        val player = compose.onNodeWithTag("library-mini-player")
            .getUnclippedBoundsInRoot()
        assertInside(player, TRACK_TITLE)
        assertInside(player, TRACK_ARTIST)

        val title = compose.onNodeWithText(TRACK_TITLE, useUnmergedTree = true)
            .getUnclippedBoundsInRoot()
        val artist = compose.onNodeWithText(TRACK_ARTIST, useUnmergedTree = true)
            .getUnclippedBoundsInRoot()
        assertTrue(
            "the title needs 8 dp above it: $title inside $player",
            title.top - player.top >= 8.dp,
        )
        assertTrue(
            "the artist needs 8 dp below it: $artist inside $player",
            player.bottom - artist.bottom >= 8.dp,
        )
    }

    @Test
    fun nonlinearScaleMiniPlayerUsesMeasuredTextInsteadOfTypographyEstimate() {
        val density = nonlinearDensity(fontScale = 2f)
        showMiniPlayer(fontScale = 2f, density = density)

        val player = compose.onNodeWithTag("library-mini-player")
            .getUnclippedBoundsInRoot()
        val title = compose.onNodeWithText(TRACK_TITLE, useUnmergedTree = true)
            .getUnclippedBoundsInRoot()
        val artist = compose.onNodeWithText(TRACK_ARTIST, useUnmergedTree = true)
            .getUnclippedBoundsInRoot()
        val estimatedHeightDp = with(density) {
            NocturneTypography.titleMedium.lineHeight.toDp().value +
                NocturneTypography.bodyMedium.lineHeight.toDp().value +
                16f
        }
        val measuredHeightDp = (player.bottom - player.top).value

        // This only validates that the nonlinear-density fixture diverges from
        // a line-height estimate; the two 8 dp clearance checks are the proof.
        assertTrue(
            "the fixture must separate the $estimatedHeightDp dp estimate from the " +
                "$measuredHeightDp dp measured mini-player",
            measuredHeightDp - estimatedHeightDp >= 8f,
        )
        assertTrue(title.top - player.top >= 8.dp)
        assertTrue(player.bottom - artist.bottom >= 8.dp)
    }

    @Test
    fun stackedMiniPlayerStays72DpAtNormalFontScale() {
        showMiniPlayer(fontScale = 1f, surfaceLayout = SurfaceLayout.STACKED)

        assertPlayerHeight(72f)
    }

    @Test
    fun wideShortMiniPlayerStays72DpAtNormalFontScale() {
        showMiniPlayer(fontScale = 1f, surfaceLayout = SurfaceLayout.WIDE_SHORT)

        assertPlayerHeight(72f)
    }

    @Test
    fun lastListRowClearsTheGrowingMiniPlayerAtDoubleFontScale() {
        showListAboveMiniPlayer(surfaceLayout = SurfaceLayout.STACKED)

        assertGrowingPlayerClearsList()
    }

    @Test
    @Config(sdk = [36], qualifiers = "w1000dp-h500dp-land")
    fun wideShortMiniPlayerGrowsWithoutClippingAtDoubleFontScale() {
        showListAboveMiniPlayer(surfaceLayout = SurfaceLayout.WIDE_SHORT)

        val player = compose.onNodeWithTag("library-mini-player")
            .getUnclippedBoundsInRoot()
        val title = compose.onNodeWithText(TRACK_TITLE, useUnmergedTree = true)
            .getUnclippedBoundsInRoot()
        val artist = compose.onNodeWithText(TRACK_ARTIST, useUnmergedTree = true)
            .getUnclippedBoundsInRoot()
        assertInside(player, TRACK_TITLE)
        assertInside(player, TRACK_ARTIST)
        assertTrue(
            "the title needs 8 dp above it: $title inside $player",
            title.top - player.top >= 8.dp,
        )
        assertTrue(
            "the artist needs 8 dp below it: $artist inside $player",
            player.bottom - artist.bottom >= 8.dp,
        )
        assertGrowingPlayerClearsList()
    }

    @Test
    fun navigationBarLabelsStayInsideAtDoubleFontScale() {
        showNavigationBar(fontScale = 2f)

        val bar = compose.onNodeWithTag("library-navigation-bar")
            .getUnclippedBoundsInRoot()
        libraryDestinations.forEach { destination ->
            val label = compose.onNodeWithText(destination.label, useUnmergedTree = true)
                .getUnclippedBoundsInRoot()
            assertTrue(
                "${destination.label} starts above the navigation bar: $label outside $bar",
                label.top >= bar.top,
            )
            assertTrue(
                "${destination.label} ends below the navigation bar: $label outside $bar",
                label.bottom <= bar.bottom,
            )
        }
        assertTrue(
            "the double-scale navigation bar must grow beyond its 80 dp floor: $bar",
            bar.bottom - bar.top > 80.dp,
        )
    }

    @Test
    fun navigationBarStays80DpAtNormalFontScale() {
        showNavigationBar(fontScale = 1f)

        val bar = compose.onNodeWithTag("library-navigation-bar")
            .getUnclippedBoundsInRoot()
        assertEquals("navigation bar bounds: $bar", 80f, (bar.bottom - bar.top).value, 0.1f)
    }

    private fun showListAboveMiniPlayer(surfaceLayout: SurfaceLayout) {
        compose.setContent {
            RepriseTheme(theme, darkPalette = true) {
                val currentDensity = LocalDensity.current
                CompositionLocalProvider(
                    LocalDensity provides Density(currentDensity.density, 2f),
                ) {
                    Scaffold(
                        bottomBar = {
                            LibraryBottomFrame(
                                surfaceLayout = surfaceLayout,
                                currentTrack = track,
                                playback = LibraryPlayback(),
                                progress = { 0f },
                                shownTab = { BrowseTab.TITLES },
                                selectTab = {},
                                openNowPlaying = {},
                            )
                        },
                    ) { contentPadding ->
                        Box(
                            modifier = Modifier
                                .fillMaxSize()
                                .padding(contentPadding),
                        ) {
                            Spacer(
                                modifier = Modifier
                                    .height(72.dp)
                                    .testTag("last-library-row")
                                    .align(Alignment.BottomStart),
                            )
                        }
                    }
                }
            }
        }
        compose.waitForIdle()
    }

    private fun assertGrowingPlayerClearsList() {
        val row = compose.onNodeWithTag("last-library-row").getUnclippedBoundsInRoot()
        val player = compose.onNodeWithTag("library-mini-player").getUnclippedBoundsInRoot()
        assertTrue(
            "the double-scale mini-player must grow beyond its floor: $player",
            player.bottom - player.top > 72.dp,
        )
        assertTrue("the last row must clear the mini-player: $row below $player", row.bottom <= player.top)
    }

    private fun showNavigationBar(fontScale: Float) {
        compose.setContent {
            RepriseTheme(theme, darkPalette = true) {
                val currentDensity = LocalDensity.current
                CompositionLocalProvider(
                    LocalDensity provides Density(currentDensity.density, fontScale),
                ) {
                    LibraryBottomFrame(
                        surfaceLayout = SurfaceLayout.STACKED,
                        currentTrack = null,
                        playback = LibraryPlayback(),
                        progress = { 0f },
                        shownTab = { BrowseTab.TITLES },
                        selectTab = {},
                        openNowPlaying = {},
                    )
                }
            }
        }
        compose.waitForIdle()
    }

    private fun showMiniPlayer(
        fontScale: Float,
        surfaceLayout: SurfaceLayout = SurfaceLayout.STACKED,
        density: Density? = null,
    ) {
        compose.setContent {
            RepriseTheme(theme, darkPalette = true) {
                val currentDensity = LocalDensity.current
                CompositionLocalProvider(
                    LocalDensity provides (
                        density ?: Density(
                            currentDensity.density,
                            fontScale,
                        )
                    ),
                ) {
                    LibraryBottomFrame(
                        surfaceLayout = surfaceLayout,
                        currentTrack = track,
                        playback = LibraryPlayback(),
                        progress = { 0f },
                        shownTab = { BrowseTab.TITLES },
                        selectTab = {},
                        openNowPlaying = {},
                    )
                }
            }
        }
        compose.waitForIdle()
    }

    private fun nonlinearDensity(fontScale: Float): Density {
        val application = RuntimeEnvironment.getApplication()
        val configuration = Configuration(application.resources.configuration).apply {
            this.fontScale = fontScale
        }
        return Density(application.createConfigurationContext(configuration))
    }

    private fun assertPlayerHeight(expected: Float) {
        val player = compose.onNodeWithTag("library-mini-player")
            .getUnclippedBoundsInRoot()
        assertEquals(
            "mini-player bounds: $player",
            expected,
            (player.bottom - player.top).value,
            0.1f,
        )
    }

    private fun assertInside(player: DpRect, text: String) {
        val node = compose.onNodeWithText(text, useUnmergedTree = true)
        val textBounds = node.getUnclippedBoundsInRoot()
        assertTrue(
            "$text starts above the mini-player: $textBounds outside $player",
            textBounds.top >= player.top,
        )
        assertTrue(
            "$text ends below the mini-player: $textBounds outside $player",
            textBounds.bottom <= player.bottom,
        )
        val layouts = mutableListOf<TextLayoutResult>()
        val resultRead = node.fetchSemanticsNode().config[SemanticsActions.GetTextLayoutResult]
            .action?.invoke(layouts) == true
        assertTrue("$text must expose its text layout", resultRead)
        assertFalse("$text must not overflow its measured height", layouts.single().didOverflowHeight)
    }

    private companion object {
        const val TRACK_TITLE = "Mini player title"
        const val TRACK_ARTIST = "Mini player artist"

        val theme = MobileThemeSelection(
            palette = MobileTheme.NOCTURNE,
            colorScheme = AndroidColorScheme.SYSTEM,
            dynamicAvailable = false,
        )
        val track = LibraryTrack(
            id = 1063,
            uri = "content://provider/document/mini-player.flac",
            title = TRACK_TITLE,
            artist = TRACK_ARTIST,
            album = "Album",
            durationMs = 123_000,
            playCount = 0,
            rating = 0,
        )
    }
}
