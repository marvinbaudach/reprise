package de.reprise.spike

import androidx.compose.material3.windowsizeclass.ExperimentalMaterial3WindowSizeClassApi
import androidx.compose.material3.windowsizeclass.WindowSizeClass
import androidx.compose.ui.unit.DpSize
import androidx.compose.ui.unit.dp
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
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

    /**
     * The ViewModel's own contract, not a claim about configuration changes —
     * that one is made by [MainActivityConfigurationTest], which goes through
     * the activity path a configuration change really takes.
     */
    @Test
    fun openingSearchReturnsToTitlesAndTheViewModelKeepsWhatItIsHanded() {
        val state = MobileSurfaceViewModel()

        state.selectTab(BrowseTab.ARTISTS)
        state.openSearch()
        state.updateSearch("slowdive")
        state.updateScroll(
            LibraryListKey.ARTISTS,
            LibraryScrollPosition(firstVisibleItemIndex = 18, itemOffsetFraction = 0.25f),
        )

        assertEquals(BrowseTab.TITLES, state.selectedTab)
        assertTrue(state.searchVisible)
        assertEquals("slowdive", state.searchText)
        assertEquals(
            LibraryScrollPosition(firstVisibleItemIndex = 18, itemOffsetFraction = 0.25f),
            state.scrollPosition(LibraryListKey.ARTISTS),
        )
    }

    @Test
    fun pagedInWindowsAreHandedBackOnlyWhileTheCatalogStillHasTheSameShape() {
        val state = MobileSurfaceViewModel()
        val paged = LoadedLibraryWindows(
            titles = LibraryWindow(total = 450, rows = emptyList(), hasMore = true),
            albums = LibraryWindow.empty(),
            artists = LibraryWindow.empty(),
        )
        val shape = LibraryCatalogShape(titles = 450, albums = 0, artists = 0)

        state.keepLoadedWindows(shape, paged)

        assertEquals(paged, state.loadedWindows(shape))
        assertNull(state.loadedWindows(shape.copy(titles = 451)))
    }

    @Test
    fun anAnchorBeyondTheLoadedRowsOpensAtTheTopRatherThanOnTheLastOne() {
        val deep = LibraryScrollPosition(firstVisibleItemIndex = 210, itemOffsetFraction = 0.4f)

        assertEquals(deep, deep.within(itemCount = 401))
        assertEquals(LibraryScrollPosition(), deep.within(itemCount = 201))
        assertEquals(LibraryScrollPosition(), deep.within(itemCount = 0))
    }

    @Test
    fun aMidRowAnchorIsKeptAsAFractionSoAShorterRowMeansTheSamePlace() {
        // 108 px into a 216 px row — half a title row on a 3x screen stacked.
        val anchor = libraryScrollPosition(
            firstVisibleItemIndex = 7,
            firstVisibleItemScrollOffsetPx = 108,
            itemHeightPx = 216,
        )

        assertEquals(0.5f, anchor.itemOffsetFraction, 0.0001f)
        assertEquals(108, anchor.offsetPxIn(itemHeightPx = 216))
        // The same half row, in the 64 dp row the wide-short arrangement uses.
        assertEquals(96, anchor.offsetPxIn(itemHeightPx = 192))
    }

    @Test
    fun aListThatHasNotMeasuredARowYetReportsNoOffsetRatherThanDividingByIt() {
        val anchor = libraryScrollPosition(
            firstVisibleItemIndex = 3,
            firstVisibleItemScrollOffsetPx = 40,
            itemHeightPx = 0,
        )

        assertEquals(3, anchor.firstVisibleItemIndex)
        assertEquals(0f, anchor.itemOffsetFraction, 0.0001f)
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
