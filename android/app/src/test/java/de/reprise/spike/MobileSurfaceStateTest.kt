package de.reprise.spike

import androidx.compose.material3.windowsizeclass.ExperimentalMaterial3WindowSizeClassApi
import androidx.compose.material3.windowsizeclass.WindowSizeClass
import androidx.compose.ui.unit.DpSize
import androidx.compose.ui.unit.dp
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class MobileSurfaceStateTest {
    @Test
    @OptIn(ExperimentalMaterial3WindowSizeClassApi::class)
    fun frame17aSelectsTheWideShortSurfaceFromItsWindowSizeClass() {
        val frame17a = WindowSizeClass.calculateFromSize(DpSize(916.dp, 412.dp))
        val portrait = WindowSizeClass.calculateFromSize(DpSize(412.dp, 916.dp))

        assertEquals(SurfaceLayout.WIDE_SHORT, surfaceLayoutFor(frame17a))
        assertEquals(SurfaceLayout.STACKED, surfaceLayoutFor(portrait))
    }

    @Test
    fun theMeasuredPortraitMetricsStaySeparateFromLandscape() {
        assertEquals(
            LibraryFrameMetrics(
                topAppBarHeightDp = 64,
                filterChipHeightDp = 32,
                trackRowHeightDp = 72,
                trackCoverSizeDp = 56,
                miniPlayerHeightDp = 72,
                navigationBarHeightDp = 80,
                navigationRailWidthDp = 80,
                navigationRailIndicatorWidthDp = 56,
                navigationRailIndicatorHeightDp = 32,
                listColumns = 1,
                listColumnGapDp = 0,
            ),
            libraryFrameMetrics(SurfaceLayout.STACKED),
        )
        assertEquals(52, libraryFrameMetrics(SurfaceLayout.WIDE_SHORT).topAppBarHeightDp)
        assertEquals(64, libraryFrameMetrics(SurfaceLayout.WIDE_SHORT).trackRowHeightDp)
        assertEquals(2, libraryFrameMetrics(SurfaceLayout.WIDE_SHORT).listColumns)
        assertEquals(16, libraryFrameMetrics(SurfaceLayout.WIDE_SHORT).listColumnGapDp)
        assertEquals(326, nowPlayingMetrics(SurfaceLayout.WIDE_SHORT).coverSizeDp)
        assertEquals(64, nowPlayingMetrics(SurfaceLayout.WIDE_SHORT).playButtonSizeDp)
    }

    @Test
    fun onlyConfigurationDurableSurfaceStateLivesInTheViewModel() {
        val state = MobileSurfaceViewModel()

        state.selectTab(BrowseTab.ARTISTS)
        state.openSearch()
        state.updateSearch("slowdive")
        state.updateScroll(
            LibraryListKey.ARTISTS,
            LibraryScrollPosition(firstVisibleItemIndex = 18, firstVisibleItemScrollOffset = 7),
        )

        assertEquals(BrowseTab.TITLES, state.selectedTab)
        assertTrue(state.searchVisible)
        assertEquals("slowdive", state.searchText)
        assertEquals(
            LibraryScrollPosition(firstVisibleItemIndex = 18, firstVisibleItemScrollOffset = 7),
            state.scrollPosition(LibraryListKey.ARTISTS),
        )
    }

    @Test
    fun anInterruptedScrubKeepsItsTimeAcrossAWidthChangeWithoutSeeking() {
        val state = MobileSurfaceViewModel()
        val trackId = 830L

        state.acceptPlaybackSnapshot(trackId, positionMs = 12_000)
        state.dragTo(trackId, positionMs = 48_000)
        state.acceptPlaybackSnapshot(trackId, positionMs = 13_000)

        val beforeTurn = state.seekPosition(trackId, fallbackPositionMs = 13_000)
        val oldHeadPx = beforeTurn.fractionOf(durationMs = 120_000) * 240f
        val newHeadPx = beforeTurn.fractionOf(durationMs = 120_000) * 480f

        assertEquals(48_000, beforeTurn.positionMs)
        assertTrue(beforeTurn.isDragging)
        assertEquals(96f, oldHeadPx, 0.001f)
        assertEquals(192f, newHeadPx, 0.001f)

        val released = state.releaseScrub(trackId)
        assertEquals(48_000, released.positionMs)
        assertFalse(released.isDragging)
    }
}
