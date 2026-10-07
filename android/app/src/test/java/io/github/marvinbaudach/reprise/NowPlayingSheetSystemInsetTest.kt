package io.github.marvinbaudach.reprise

import androidx.activity.ComponentActivity
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.dp
import androidx.core.graphics.Insets
import androidx.core.view.WindowInsetsCompat
import io.github.marvinbaudach.reprise.ui.theme.RepriseTheme
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.reprise_android_ffi.AndroidColorScheme

/**
 * The system's gesture zone must never be drawn over the Now Playing transport
 * (#1178).
 *
 * The root consumes only the status bar, so the sheet is the one place that
 * spends the navigation-bar inset beneath its transport row: the bottom edge
 * for gesture navigation, and the side a three-button bar occupies in
 * landscape. The wide-short sheet sits beside the library's rail, which already
 * spends the start inset.
 */
@RunWith(RobolectricTestRunner::class)
class NowPlayingSheetSystemInsetTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    @Config(sdk = [36], qualifiers = "w916dp-h412dp-land")
    fun wideShortTransportClearsTheBottomInset() {
        showSheet(SurfaceLayout.WIDE_SHORT)
        applyNavigationBarInset()

        assertTransportEndsAboveTheBottomInset()
    }

    @Test
    @Config(sdk = [36], qualifiers = "w916dp-h340dp-land")
    fun wideShortTransportClearsTheBottomInsetAtDoubleFontScale() {
        showSheet(SurfaceLayout.WIDE_SHORT, fontScale = 2f)
        applyNavigationBarInset()

        assertTransportEndsAboveTheBottomInset()
    }

    @Test
    @Config(sdk = [36], qualifiers = "w916dp-h412dp-land")
    fun wideShortTransportSitsOnItsOwnPaddingWithoutAnInset() {
        showSheet(SurfaceLayout.WIDE_SHORT)

        assertEquals(WIDE_SHORT_PADDING_DP, gapBelowTransport(), 0.1f)
    }

    @Test
    @Config(sdk = [36], qualifiers = "w916dp-h412dp-land")
    fun wideShortTransportClearsTheEndSideNavigationBar() {
        showSheet(SurfaceLayout.WIDE_SHORT)
        applyNavigationBarInset(end = INSET_DP)

        val root = frameBounds()
        val repeat = compose.onNodeWithContentDescription("Repeat off").getUnclippedBoundsInRoot()
        assertTrue(
            "the trailing control must end $INSET_DP dp before the end edge: $repeat in $root",
            root.right - repeat.right >= INSET_DP.dp,
        )
    }

    @Test
    @Config(sdk = [36], qualifiers = "w916dp-h412dp-land")
    fun wideShortSheetLeavesTheStartSideToTheRail() {
        showSheet(SurfaceLayout.WIDE_SHORT)
        val before = compose.onNodeWithTag("now-playing-cover").getUnclippedBoundsInRoot()
        applyNavigationBarInset(start = INSET_DP)

        val after = compose.onNodeWithTag("now-playing-cover").getUnclippedBoundsInRoot()
        assertEquals("the rail already spends the start inset", before.left.value, after.left.value, 0.1f)
    }

    @Test
    @Config(sdk = [36], qualifiers = "w412dp-h916dp-port")
    fun stackedTransportClearsTheBottomInset() {
        showSheet(SurfaceLayout.STACKED)
        applyNavigationBarInset()

        assertTransportEndsAboveTheBottomInset()
    }

    @Test
    @Config(sdk = [36], qualifiers = "w412dp-h916dp-port")
    fun stackedTransportSitsOnItsOwnPaddingWithoutAnInset() {
        showSheet(SurfaceLayout.STACKED)

        assertEquals(STACKED_PADDING_DP, gapBelowTransport(), 0.1f)
    }

    @Test
    @Config(sdk = [36], qualifiers = "w700dp-h400dp-land")
    fun stackedLandscapeTransportClearsBothSideInsets() {
        showSheet(SurfaceLayout.STACKED)
        applyNavigationBarInset(start = INSET_DP, end = INSET_DP)

        val root = frameBounds()
        val shuffle = compose.onNodeWithContentDescription("Turn shuffle on").getUnclippedBoundsInRoot()
        val repeat = compose.onNodeWithContentDescription("Repeat off").getUnclippedBoundsInRoot()
        assertTrue("$shuffle in $root", shuffle.left - root.left >= INSET_DP.dp)
        assertTrue("$repeat in $root", root.right - repeat.right >= INSET_DP.dp)
    }

    private fun frameBounds() = compose.onNodeWithTag("frame-root").getUnclippedBoundsInRoot()

    private fun gapBelowTransport(): Float {
        val transport = compose.onNodeWithTag("now-playing-transport").getUnclippedBoundsInRoot()
        return (frameBounds().bottom - transport.bottom).value
    }

    private fun assertTransportEndsAboveTheBottomInset() {
        val root = frameBounds()
        val transport = compose.onNodeWithTag("now-playing-transport").getUnclippedBoundsInRoot()
        assertTrue(
            "the transport must end $INSET_DP dp above the bottom edge: $transport in $root",
            root.bottom - transport.bottom >= INSET_DP.dp,
        )
    }

    private fun showSheet(surfaceLayout: SurfaceLayout, fontScale: Float = 1f) {
        compose.setContent {
            RepriseTheme(theme, darkPalette = true) {
                val density = LocalDensity.current
                CompositionLocalProvider(
                    LocalPlaybackControls provides DisconnectedPlaybackControls,
                    LocalDensity provides Density(density.density, fontScale),
                ) {
                    Box(Modifier.fillMaxSize().testTag("frame-root")) {
                        NowPlayingSheet(
                            track = configurationTestTrack(1178, "Gesture zone"),
                            playback = PlaybackUiState(),
                            surfaceLayout = surfaceLayout,
                            close = {},
                        )
                    }
                }
            }
        }
        compose.waitForIdle()
    }

    /**
     * What a navigation bar reports: at the bottom for gesture navigation, at
     * the [start] or [end] side for three-button navigation in landscape.
     */
    private fun applyNavigationBarInset(
        start: Float = 0f,
        end: Float = 0f,
        bottom: Float = if (start == 0f && end == 0f) INSET_DP else 0f,
    ) {
        val density = compose.activity.resources.displayMetrics.density
        val insets = WindowInsetsCompat.Builder()
            .setInsets(
                WindowInsetsCompat.Type.navigationBars(),
                Insets.of(
                    (start * density).toInt(),
                    0,
                    (end * density).toInt(),
                    (bottom * density).toInt(),
                ),
            )
            .build()
            .toWindowInsets()!!
        compose.runOnUiThread { compose.activity.window.decorView.dispatchApplyWindowInsets(insets) }
        compose.waitForIdle()
    }

    private companion object {
        const val INSET_DP = 24f
        const val WIDE_SHORT_PADDING_DP = 8f
        const val STACKED_PADDING_DP = 18f

        val theme = MobileThemeSelection(
            palette = MobileTheme.NOCTURNE,
            colorScheme = AndroidColorScheme.SYSTEM,
            dynamicAvailable = false,
        )
    }
}
