package io.github.marvinbaudach.reprise

import androidx.activity.ComponentActivity
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
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
 * The system's gesture handle must never be drawn over the mini-player (#1074).
 *
 * The root consumes only the status bar, so [LibraryBottomFrame] is the one
 * place the bottom system inset is spent: the portrait bar spends it inside the
 * navigation bar, and the landscape frame, which has no bar, has to spend it
 * under the mini-player itself.
 */
@RunWith(RobolectricTestRunner::class)
class MiniPlayerSystemInsetTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    @Config(sdk = [36], qualifiers = "w1000dp-h500dp-land")
    fun wideShortMiniPlayerClearsTheBottomSystemInset() {
        showFrame(SurfaceLayout.WIDE_SHORT)
        applyNavigationBarInset()

        val root = compose.onNodeWithTag("frame-root").getUnclippedBoundsInRoot()
        val player = compose.onNodeWithTag("library-mini-player").getUnclippedBoundsInRoot()
        assertTrue(
            "the mini-player must end $INSET_DP dp above the bottom edge: $player in $root",
            root.bottom - player.bottom >= INSET_DP.dp,
        )
    }

    @Test
    @Config(sdk = [36], qualifiers = "w1000dp-h500dp-land")
    fun wideShortFrameStillSitsOnTheBottomEdgeWithoutAnInset() {
        showFrame(SurfaceLayout.WIDE_SHORT)

        val root = compose.onNodeWithTag("frame-root").getUnclippedBoundsInRoot()
        val player = compose.onNodeWithTag("library-mini-player").getUnclippedBoundsInRoot()
        assertEquals("$player in $root", 0f, (root.bottom - player.bottom).value, 0.1f)
    }

    @Test
    @Config(sdk = [36], qualifiers = "w412dp-h916dp-port")
    fun stackedFrameSpendsTheBottomSystemInsetExactlyOnce() {
        showFrame(SurfaceLayout.STACKED)
        applyNavigationBarInset()

        val root = compose.onNodeWithTag("frame-root").getUnclippedBoundsInRoot()
        val bar = compose.onNodeWithTag("library-navigation-bar").getUnclippedBoundsInRoot()
        val player = compose.onNodeWithTag("library-mini-player").getUnclippedBoundsInRoot()
        assertEquals("$bar in $root", 80f + INSET_DP, (bar.bottom - bar.top).value, 0.1f)
        assertEquals("$bar in $root", 0f, (root.bottom - bar.bottom).value, 0.1f)
        assertEquals("$player above $bar", 0f, (bar.top - player.bottom).value, 0.1f)
    }

    private fun showFrame(surfaceLayout: SurfaceLayout) {
        compose.setContent {
            RepriseTheme(theme, darkPalette = true) {
                Box(
                    modifier = Modifier.fillMaxSize().testTag("frame-root"),
                    contentAlignment = Alignment.BottomCenter,
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

    /** What a gesture-navigation device reports: a bottom navigation-bar inset. */
    private fun applyNavigationBarInset() {
        val density = compose.activity.resources.displayMetrics.density
        val insets = WindowInsetsCompat.Builder()
            .setInsets(
                WindowInsetsCompat.Type.navigationBars(),
                Insets.of(0, 0, 0, (INSET_DP * density).toInt()),
            )
            .build()
            .toWindowInsets()!!
        compose.runOnUiThread { compose.activity.window.decorView.dispatchApplyWindowInsets(insets) }
        compose.waitForIdle()
    }

    private companion object {
        const val INSET_DP = 24f

        val theme = MobileThemeSelection(
            palette = MobileTheme.NOCTURNE,
            colorScheme = AndroidColorScheme.SYSTEM,
            dynamicAvailable = false,
        )
        val track = LibraryTrack(
            id = 1074,
            uri = "content://provider/document/gesture-handle.flac",
            title = "Gesture handle",
            artist = "Artist",
            album = "Album",
            durationMs = 58_000,
            playCount = 0,
            rating = 0,
        )
    }
}
