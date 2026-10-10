package io.github.marvinbaudach.reprise

import androidx.activity.ComponentActivity
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.DpRect
import androidx.compose.ui.unit.dp
import androidx.core.graphics.Insets
import androidx.core.view.WindowInsetsCompat
import io.github.marvinbaudach.reprise.ui.theme.RepriseTheme
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import uniffi.reprise_android_ffi.AndroidColorScheme

/**
 * On a small phone at a large font scale the stacked Now Playing sheet keeps
 * its title, artist, seek labels and transport row apart (#1190).
 *
 * The qualifiers are the phone the issue was measured on: 1080x1920 at 480 dpi
 * is 360x640 dp. Its status bar is spent by the root, so the sheet is laid
 * out below [STATUS_BAR_DP]; its gesture navigation bar is applied as the
 * bottom inset the transport row clears. The control cases pin the positions
 * a roomy screen has always had: only a screen that is too short for the
 * measured heights may move them.
 */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class NowPlayingSmallPhoneLayoutTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    @Config(sdk = [36], qualifiers = "w360dp-h640dp-port-xxhdpi")
    fun nav_21_a_small_phone_at_double_font_scale_keeps_every_block_apart() {
        showSheet(fontScale = 2f)

        assertBlocksDoNotOverlap()
        assertTransportStaysPut()
        assertCoverKeepsMostOfItsSize()
    }

    @Test
    @Config(sdk = [36], qualifiers = "w360dp-h640dp-port-xxhdpi")
    fun nav_21_a_small_phone_at_normal_font_scale_keeps_every_block_apart() {
        showSheet(fontScale = 1f)

        assertBlocksDoNotOverlap()
    }

    @Test
    @Config(sdk = [36], qualifiers = "w412dp-h916dp-port")
    fun nav_21_a_tall_phone_at_double_font_scale_keeps_every_block_apart() {
        showSheet(fontScale = 2f)

        assertBlocksDoNotOverlap()
    }

    @Test
    @Config(sdk = [36], qualifiers = "w412dp-h916dp-port")
    fun nav_21_a_tall_phone_at_normal_font_scale_keeps_its_fractional_positions() {
        showSheet(fontScale = 1f, title = SHORT_TITLE)

        assertFractionalPositions()
        assertBlocksDoNotOverlap()
    }

    private fun assertFractionalPositions() {
        val scene = bounds("now-playing-scene")
        val cover = bounds("now-playing-scene-cover")
        val title = bounds("now-playing-title")
        val seek = bounds("now-playing-seek")
        val sheetHeight = (scene.bottom - scene.top).value

        assertEquals(
            "the cover stays centred on 34% of the sheet",
            scene.top.value + sheetHeight * 0.34f,
            (cover.top.value + cover.bottom.value) / 2f,
            TOLERANCE_DP,
        )
        assertEquals("the cover keeps its size", 272f, (cover.right - cover.left).value, TOLERANCE_DP)
        assertEquals(
            "the title block stays 156 dp below the cover's centre",
            scene.top.value + sheetHeight * 0.34f + 156f,
            title.top.value,
            TOLERANCE_DP,
        )
        assertEquals(
            "the seek bar stays at 69% of the sheet, inside its 48 dp touch area",
            scene.top.value + sheetHeight * 0.69f + SEEK_TOUCH_INSET_DP,
            seek.top.value,
            TOLERANCE_DP,
        )
    }

    /** FB-18 floats the Undo snackbar over the transport row's top, which must not move. */
    private fun assertTransportStaysPut() {
        val frame = bounds("frame-root")
        val transport = bounds("now-playing-transport")
        assertEquals(
            "the transport row keeps its place above the bottom edge and the navigation bar",
            nowPlayingTransportTop(SurfaceLayout.STACKED).value + NAVIGATION_BAR_DP,
            (frame.bottom - transport.top).value,
            TOLERANCE_DP,
        )
    }

    private fun assertCoverKeepsMostOfItsSize() {
        val cover = bounds("now-playing-scene-cover")
        assertTrue(
            "the cover gave up more than it should: $cover",
            (cover.right - cover.left).value >= 272f * 0.7f - TOLERANCE_DP,
        )
    }

    private fun assertBlocksDoNotOverlap() {
        val blocks = linkedMapOf(
            "cover" to bounds("now-playing-scene-cover"),
            "title" to bounds("now-playing-title"),
            "artist" to bounds("now-playing-artist"),
            "seek" to bounds("now-playing-seek"),
            "position" to bounds("now-playing-position"),
            "remaining" to bounds("now-playing-remaining"),
            "shuffle" to compose.onNodeWithContentDescription("Turn shuffle on").getUnclippedBoundsInRoot(),
            "play" to bounds("now-playing-play"),
            "repeat" to compose.onNodeWithContentDescription("Repeat off").getUnclippedBoundsInRoot(),
        )
        val names = blocks.keys.toList()
        for (i in names.indices) {
            for (j in i + 1 until names.size) {
                val a = blocks.getValue(names[i])
                val b = blocks.getValue(names[j])
                assertFalse(
                    "${names[i]} $a overlaps ${names[j]} $b; all: $blocks",
                    a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom,
                )
            }
        }
    }

    private fun bounds(tag: String): DpRect = compose.onNodeWithTag(tag).getUnclippedBoundsInRoot()

    private fun showSheet(fontScale: Float, title: String = LONG_TITLE) {
        compose.setContent {
            RepriseTheme(theme, darkPalette = true) {
                val density = LocalDensity.current
                CompositionLocalProvider(
                    LocalPlaybackControls provides DisconnectedPlaybackControls,
                    LocalDensity provides Density(density.density, fontScale),
                ) {
                    Box(Modifier.fillMaxSize().padding(top = STATUS_BAR_DP.dp).testTag("frame-root")) {
                        NowPlayingSheet(
                            track = configurationTestTrack(1190, title),
                            playback = PlaybackUiState(),
                            surfaceLayout = SurfaceLayout.STACKED,
                            close = {},
                        )
                    }
                }
            }
        }
        compose.waitForIdle()
        applyNavigationBarInset()
    }

    private fun applyNavigationBarInset() {
        val density = compose.activity.resources.displayMetrics.density
        val insets = WindowInsetsCompat.Builder()
            .setInsets(
                WindowInsetsCompat.Type.navigationBars(),
                Insets.of(0, 0, 0, (NAVIGATION_BAR_DP * density).toInt()),
            )
            .build()
            .toWindowInsets()!!
        compose.runOnUiThread { compose.activity.window.decorView.dispatchApplyWindowInsets(insets) }
        compose.waitForIdle()
    }

    private companion object {
        const val LONG_TITLE =
            "Everything That Happens Will Happen Today (Extended Remaster Version)"
        const val SHORT_TITLE = "Gesture zone"
        const val STATUS_BAR_DP = 42
        const val NAVIGATION_BAR_DP = 24f
        const val TOLERANCE_DP = 1f

        /** The slider's visible bounds sit this far inside its 48 dp minimum touch area. */
        const val SEEK_TOUCH_INSET_DP = 8f

        val theme = MobileThemeSelection(
            palette = MobileTheme.NOCTURNE,
            colorScheme = AndroidColorScheme.SYSTEM,
            dynamicAvailable = false,
        )
    }
}
