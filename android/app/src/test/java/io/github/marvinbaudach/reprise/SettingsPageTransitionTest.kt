package io.github.marvinbaudach.reprise

import androidx.activity.ComponentActivity
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.PixelMap
import androidx.compose.ui.graphics.toPixelMap
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import io.github.marvinbaudach.reprise.settings.SETTINGS_PAGE_SLIDE_MS
import io.github.marvinbaudach.reprise.settings.SettingsNavigation
import io.github.marvinbaudach.reprise.ui.theme.RepriseTheme
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import uniffi.reprise_android_ffi.AndroidColorScheme
import kotlin.math.roundToInt

/**
 * The settings page in front hides the overview beneath it, whether it is
 * arriving or leaving. Seen on a device: going back from a page to the
 * overview showed the page's rows *through* the overview's rows for most of
 * a second, because the navigation graph crossfaded two pages that painted no
 * background of their own — the only opaque surface sat outside the graph.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36], qualifiers = "w412dp-h916dp-port")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class SettingsPageTransitionTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    fun theLeavingPageStaysOpaqueWhileTheOverviewComesBack() {
        mountSettings()
        compose.onNodeWithContentDescription("Open Audio").performClick()
        compose.onNodeWithTag("settings-page-audio").assertIsDisplayed()
        val atRest = captureHost()

        compose.mainClock.autoAdvance = false
        compose.onNodeWithContentDescription("Back to Settings").performClick()
        val (midway, shift) = captureMidTransition()

        assertPageIntact(atRest, midway, shift)
    }

    @Test
    fun theArrivingPageCoversTheOverviewOnItsWayIn() {
        mountSettings()

        compose.mainClock.autoAdvance = false
        compose.onNodeWithContentDescription("Open Audio").performClick()
        val (midway, shift) = captureMidTransition()
        assertTrue("the page is still arriving (shift $shift)", shift > 0)

        compose.mainClock.autoAdvance = true
        compose.waitForIdle()
        val atRest = captureHost()

        assertPageIntact(atRest, midway, shift)
    }

    /**
     * Advances to the middle of the slide and captures the host, together
     * with how far the audio page then stands from the left edge, in pixels.
     */
    private fun captureMidTransition(): Pair<PixelMap, Int> {
        compose.mainClock.advanceTimeByFrame()
        compose.mainClock.advanceTimeBy(SETTINGS_PAGE_SLIDE_MS / 2L)

        // Both pages are composed: the transition is in flight, not over.
        compose.onAllNodesWithTag("settings-overview-row").assertCountEquals(5)
        val page = compose.onNodeWithTag("settings-page-audio").getUnclippedBoundsInRoot()
        val shift = with(compose.density) { page.left.toPx() }.roundToInt()
        return captureHost() to shift
    }

    /**
     * Compares the page's own pixels, where the page stands midway, with the
     * same pixels where it stands at rest. A page drawn without a background
     * lets the overview show through, and those pixels stop matching.
     */
    private fun assertPageIntact(atRest: PixelMap, midway: PixelMap, shift: Int) {
        assertEquals("the host keeps its size", atRest.width, midway.width)
        assertTrue("the page is on screen (shift $shift)", shift in 0 until midway.width)
        var bleeding = 0
        for (y in 0 until midway.height) {
            for (x in 0 until midway.width - shift) {
                if (atRest[x, y] != midway[x + shift, y]) bleeding++
            }
        }
        assertEquals(
            "pixels under the page in front that do not match the page at rest",
            0,
            bleeding,
        )
    }

    private fun captureHost(): PixelMap =
        compose.onNodeWithTag(HOST_TAG).captureToImage().toPixelMap()

    private fun mountSettings() {
        val theme = MobileThemeSelection(
            palette = MobileTheme.NOCTURNE,
            colorScheme = AndroidColorScheme.SYSTEM,
            dynamicAvailable = false,
        )
        compose.setContent {
            RepriseTheme(theme, darkPalette = true) {
                // The same opaque host the overlay in BrowseScreen draws the
                // graph inside: it is what a transparent page would let through.
                Surface(
                    modifier = Modifier.fillMaxSize().testTag(HOST_TAG),
                    color = MaterialTheme.colorScheme.background,
                ) {
                    SettingsNavigation(
                        state = PlaybackSettingsUiState(
                            equalizerEnabled = true,
                            gaplessEnabled = true,
                            equalizerBands = emptyList(),
                        ),
                        titleCount = 1_824,
                        albumCount = 143,
                        artistCount = 92,
                        folderName = "Music",
                        themeSelection = theme,
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
    }

    private companion object {
        const val HOST_TAG = "settings-host"
    }
}
