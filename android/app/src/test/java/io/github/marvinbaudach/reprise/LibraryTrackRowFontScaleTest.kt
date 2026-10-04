package io.github.marvinbaudach.reprise

import android.content.res.Configuration
import androidx.activity.ComponentActivity
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performTouchInput
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
class LibraryTrackRowFontScaleTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    fun trackRowsDoNotClipTextAtDoubleFontScale() {
        showTrackRows(fontScale = 2f)

        val row = compose.onNodeWithTag("library-track-row-901")
            .getUnclippedBoundsInRoot()
        assertInside(row, "Title")
        assertInside(row, "A • B")
        assertInside(row, "2:03")
    }

    @Test
    fun doubleScaleRowKeepsEightDpPaddingAroundItsMeasuredText() {
        showTrackRows(fontScale = 2f)

        val row = compose.onNodeWithTag("library-track-row-901")
            .getUnclippedBoundsInRoot()
        val title = compose.onNodeWithText("Title", useUnmergedTree = true)
            .getUnclippedBoundsInRoot()
        val subtitle = compose.onNodeWithText("A • B", useUnmergedTree = true)
            .getUnclippedBoundsInRoot()
        assertTrue(
            "the title needs 8 dp above it: $title inside $row",
            title.top - row.top >= 8.dp,
        )
        assertTrue(
            "the subtitle needs 8 dp below it: $subtitle inside $row",
            row.bottom - subtitle.bottom >= 8.dp,
        )
    }

    @Test
    fun nonlinearScaleRowUsesMeasuredTextInsteadOfTypographyEstimate() {
        val density = nonlinearDensity(fontScale = 2f)
        showTrackRows(fontScale = 2f, density = density)

        val row = compose.onNodeWithTag("library-track-row-901")
            .getUnclippedBoundsInRoot()
        val title = compose.onNodeWithText("Title", useUnmergedTree = true)
            .getUnclippedBoundsInRoot()
        val subtitle = compose.onNodeWithText("A • B", useUnmergedTree = true)
            .getUnclippedBoundsInRoot()
        val estimatedHeightDp = with(density) {
            NocturneTypography.titleMedium.lineHeight.toDp().value +
                NocturneTypography.bodyMedium.lineHeight.toDp().value +
                16f
        }
        val measuredHeightDp = (row.bottom - row.top).value

        assertTrue(
            "the fixture must separate the $estimatedHeightDp dp estimate from the " +
                "$measuredHeightDp dp measured row",
            measuredHeightDp - estimatedHeightDp >= 8f,
        )
        assertTrue(title.top - row.top >= 8.dp)
        assertTrue(row.bottom - subtitle.bottom >= 8.dp)
    }

    @Test
    fun stackedRowKeepsIts72DpMinimumAtNormalFontScale() {
        showTrackRows(fontScale = 1f, surfaceLayout = SurfaceLayout.STACKED)

        val row = compose.onNodeWithTag("library-track-row-901")
            .getUnclippedBoundsInRoot()
        assertEquals("row bounds: $row", 72f, (row.bottom - row.top).value, 0.1f)
    }

    @Test
    fun wideShortRowKeepsIts64DpMinimumAtNormalFontScale() {
        showTrackRows(fontScale = 1f, surfaceLayout = SurfaceLayout.WIDE_SHORT)

        val row = compose.onNodeWithTag("library-track-row-901")
            .getUnclippedBoundsInRoot()
        assertEquals("row bounds: $row", 64f, (row.bottom - row.top).value, 0.1f)
    }

    @Test
    fun doubleScaleQueueUsesTheRowHeightForMotionAndDrop() {
        val moves = mutableListOf<Triple<Int, Long, Int>>()
        val tracks = listOf(track(901, "First row"), track(902, "Second row"))
        showTrackRows(
            fontScale = 2f,
            tracks = tracks,
            queueActions = QueueRowActions(
                play = { _, _ -> },
                move = { from, trackId, to -> moves += Triple(from, trackId, to) },
                remove = { _, _ -> },
            ),
        )

        val firstRow = compose.onNodeWithTag("queue-track-row-901")
        val handle = compose.onNodeWithTag("queue-drag-handle-901", useUnmergedTree = true)
        val rowBoundsDp = firstRow.getUnclippedBoundsInRoot()
        val rowHeightDp = (rowBoundsDp.bottom - rowBoundsDp.top).value
        val rowHeightPx = firstRow.fetchSemanticsNode().boundsInRoot.height
        assertTrue("the double-scale row must grow beyond its 72 dp base", rowHeightDp > 72f)

        compose.mainClock.autoAdvance = false
        try {
            val restingSecondTop = compose.onNodeWithTag("queue-track-row-902")
                .fetchSemanticsNode().boundsInRoot.top
            handle.performTouchInput {
                down(center)
                moveBy(Offset(0f, rowHeightPx * 0.49f))
            }
            compose.waitForIdle()
            compose.mainClock.advanceTimeBy(QUEUE_DRAG_NEIGHBOUR_MS * 3L)
            assertEquals(
                "less than half the measured row height must not cross a slot",
                restingSecondTop,
                compose.onNodeWithTag("queue-track-row-902")
                    .fetchSemanticsNode().boundsInRoot.top,
                1f,
            )

            handle.performTouchInput {
                moveBy(Offset(0f, rowHeightPx * 0.51f))
            }
            compose.waitForIdle()
            compose.mainClock.advanceTimeBy(QUEUE_DRAG_NEIGHBOUR_MS * 3L)

            assertEquals(
                restingSecondTop - rowHeightPx,
                compose.onNodeWithTag("queue-track-row-902")
                    .fetchSemanticsNode().boundsInRoot.top,
                1f,
            )

            handle.performTouchInput { up() }
            compose.waitForIdle()
            compose.mainClock.advanceTimeBy(QUEUE_DRAG_DROP_MS * 2L)
        } finally {
            compose.mainClock.autoAdvance = true
        }

        compose.waitForIdle()
        assertEquals(listOf(Triple(0, 901L, 1)), moves)
    }

    private fun showTrackRows(
        fontScale: Float,
        tracks: List<LibraryTrack> = listOf(track(901, "Title")),
        queueActions: QueueRowActions? = null,
        surfaceLayout: SurfaceLayout = SurfaceLayout.STACKED,
        density: Density? = null,
    ) {
        val surfaceState = MobileSurfaceViewModel()
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
                    TrackRows(
                        surfaceLayout = surfaceLayout,
                        surfaceState = surfaceState,
                        listKey = LibraryListKey.TITLES,
                        tracks = LibraryWindow(
                            total = tracks.size.toLong(),
                            rows = tracks,
                            hasMore = false,
                        ),
                        playback = PlaybackUiState().libraryPlayback(),
                        lastRequestedOffset = null,
                        play = {},
                        loadMore = {},
                        queueActions = queueActions,
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

    private fun assertInside(row: DpRect, text: String) {
        val node = compose.onNodeWithText(text, useUnmergedTree = true)
        val textBounds = node.getUnclippedBoundsInRoot()
        assertTrue("$text starts above its row: $textBounds outside $row", textBounds.top >= row.top)
        assertTrue(
            "$text ends below its row: $textBounds outside $row",
            textBounds.bottom <= row.bottom,
        )
        val layouts = mutableListOf<TextLayoutResult>()
        val resultRead = node.fetchSemanticsNode().config[SemanticsActions.GetTextLayoutResult]
            .action?.invoke(layouts) == true
        assertTrue("$text must expose its text layout", resultRead)
        assertFalse("$text must not overflow its measured height", layouts.single().didOverflowHeight)
    }

    private companion object {
        fun track(id: Long, title: String) = LibraryTrack(
            id = id,
            uri = "content://provider/document/$id.flac",
            title = title,
            artist = "A",
            album = "B",
            durationMs = 123_000,
            playCount = 0,
            rating = 0,
        )

        val theme = MobileThemeSelection(
            palette = MobileTheme.NOCTURNE,
            colorScheme = AndroidColorScheme.SYSTEM,
            dynamicAvailable = false,
        )
    }
}
