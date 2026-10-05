package io.github.marvinbaudach.reprise

import androidx.activity.ComponentActivity
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.unit.dp
import io.github.marvinbaudach.reprise.ui.theme.RepriseTheme
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.reprise_android_ffi.AndroidColorScheme

/**
 * The clearance the snackbar keeps over the Now Playing sheet is derived from
 * the sheet's layout constants, which this surface does not own. Measuring the
 * real transport row here is what keeps the derivation honest.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class UndoSnackbarClearanceTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private val harness = DeletionHarness()
    private val theme = MobileThemeSelection(
        palette = MobileTheme.NOCTURNE,
        colorScheme = AndroidColorScheme.SYSTEM,
        dynamicAvailable = false,
    )

    @Test
    @Config(qualifiers = "w412dp-h916dp-port")
    fun overTheStackedSheetTheSnackbarSitsJustAboveTheTransportRow() =
        assertSitsAboveTheTransport(SurfaceLayout.STACKED)

    @Test
    @Config(qualifiers = "w916dp-h412dp-land")
    fun overTheWideShortSheetTheSnackbarSitsJustAboveTheTransportRow() =
        assertSitsAboveTheTransport(SurfaceLayout.WIDE_SHORT)

    @Test
    fun withoutTheSheetTheLibraryFrameSetsTheClearance() {
        assertEquals(80.dp, undoSnackbarClearance(false, SurfaceLayout.STACKED, 80.dp))
        assertEquals(
            nowPlayingTransportTop(SurfaceLayout.STACKED),
            undoSnackbarClearance(true, SurfaceLayout.STACKED, 80.dp),
        )
    }

    private fun assertSitsAboveTheTransport(layout: SurfaceLayout) {
        val queue = FakeQueueControls(listOf(10, 11))
        compose.setContent {
            RepriseTheme(theme, darkPalette = true) {
                CompositionLocalProvider(LocalPlaybackControls provides queue) {
                    Box(Modifier.fillMaxSize()) {
                        NowPlayingSheet(
                            track = configurationTestTrack(41, "Song"),
                            playback = PlaybackUiState(),
                            surfaceLayout = layout,
                            surfaceState = harness.surface,
                            close = {},
                        )
                        UndoSnackbarHost(harness.surface.pendingDeletions) {
                            undoSnackbarClearance(true, layout, 0.dp)
                        }
                    }
                }
            }
        }
        compose.runOnIdle { harness.surface.pendingDeletions.begin(listOf(10), queue) }
        compose.awaitText("1 track will be deleted")

        val transportTop = compose.onNodeWithTag("now-playing-transport")
            .getUnclippedBoundsInRoot().top.value
        val hostBottom = compose.onNodeWithTag("undo-snackbar-host")
            .getUnclippedBoundsInRoot().bottom.value

        // The host leaves an 8 dp gap over what it clears.
        assertEquals(transportTop - 8f, hostBottom, 1.5f)
    }
}
