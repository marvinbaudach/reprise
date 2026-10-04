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
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.DpRect
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.marvinbaudach.reprise.ui.theme.NocturneTypography
import io.github.marvinbaudach.reprise.ui.theme.RepriseTheme
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import uniffi.reprise_android_ffi.AndroidColorScheme
import kotlin.math.abs

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
    fun rowHeightUsesNonlinearDensityConversionAtDoubleFontScale() {
        val density = nonlinearDensity(fontScale = 2f)
        showTrackRows(fontScale = 2f, density = density)

        val expectedHeightDp = with(density) {
            NocturneTypography.titleMedium.lineHeight.toDp().value +
                NocturneTypography.bodyMedium.lineHeight.toDp().value +
                16f
        }
        val linearHeightDp = (
            NocturneTypography.titleMedium.lineHeight.value +
                NocturneTypography.bodyMedium.lineHeight.value
            ) * density.fontScale + 16f
        val nonlinearGapDp = abs(expectedHeightDp - linearHeightDp)
        assertTrue(
            "The fixture no longer distinguishes nonlinear from linear scaling: " +
                "$nonlinearGapDp dp is below the $NONLINEAR_MINIMUM_GAP_DP dp minimum",
            nonlinearGapDp >= NONLINEAR_MINIMUM_GAP_DP,
        )
        val row = compose.onNodeWithTag("library-track-row-901")
            .getUnclippedBoundsInRoot()
        assertEquals(expectedHeightDp, (row.bottom - row.top).value, 0.1f)
    }

    @Test
    fun effectiveHeightKeepsBaseRowsAtNormalFontScale() {
        listOf(72, 64).forEach { baseHeightDp ->
            assertEquals(
                baseHeightDp.toFloat(),
                effectiveTrackRowHeightDp(
                    baseHeightDp = baseHeightDp,
                    density = Density(density = 1f, fontScale = 1f),
                    titleLineHeight = NocturneTypography.titleMedium.lineHeight,
                    subtitleLineHeight = NocturneTypography.bodyMedium.lineHeight,
                ),
                0f,
            )
        }
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
                moveBy(Offset(0f, rowHeightPx))
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
        density: Density? = null,
    ) {
        val surfaceState = MobileSurfaceViewModel()
        compose.setContent {
            RepriseTheme(theme, darkPalette = true) {
                val currentDensity = LocalDensity.current
                CompositionLocalProvider(
                    LocalDensity provides (
                        density ?: LinearTestDensity(currentDensity.density, fontScale)
                    ),
                ) {
                    TrackRows(
                        surfaceLayout = SurfaceLayout.STACKED,
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
        private const val NONLINEAR_MINIMUM_GAP_DP = 8f

        private class LinearTestDensity(
            override val density: Float,
            override val fontScale: Float,
        ) : Density {
            override fun TextUnit.toDp(): Dp = (value * fontScale).dp

            override fun Dp.toSp(): TextUnit = (value / fontScale).sp
        }

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
