package de.reprise.spike

import androidx.compose.material3.windowsizeclass.ExperimentalMaterial3WindowSizeClassApi
import androidx.compose.material3.windowsizeclass.WindowSizeClass
import androidx.compose.ui.unit.DpSize
import androidx.compose.ui.unit.dp
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import java.util.concurrent.TimeUnit
import android.graphics.Bitmap
import uniffi.reprise_android_ffi.AndroidArtworkSize
import uniffi.reprise_android_ffi.AndroidRepeatMode

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class MobileSurfaceStateTest {
    @Test
    fun aNewCurrentTrackPrefetchesTheFollowingTwoCoversAndFogTextures() {
        val cache = ArtworkCache()
        val tracks = listOf(prefetchTrack(2), prefetchTrack(3))
        val controls = object : PlaybackControls {
            override fun togglePause() = Unit
            override fun next() = Unit
            override fun previous() = Unit
            override fun seekTo(positionMs: Long) = Unit
            override fun setShuffle(enabled: Boolean) = Unit
            override fun setRepeat(mode: AndroidRepeatMode) = Unit
            override fun setFavourite(trackId: Long, favourite: Boolean, report: (String?) -> Unit) =
                report(null)
            override fun loadUpcomingTracks(
                window: LibraryWindowRange,
                report: (Result<LibraryWindow<LibraryTrack>>) -> Unit,
            ) {
                assertEquals(LibraryWindowRange(0, 2), window)
                report(Result.success(LibraryWindow(2, tracks, hasMore = false)))
            }
        }
        val bitmap = Bitmap.createBitmap(8, 8, Bitmap.Config.ARGB_8888)
        val artwork = TrackArtwork(
            resolve = { uri, _ -> uri },
            decode = { bitmap },
            cache = cache,
            onMainThread = { work -> work() },
        )
        val state = MobileSurfaceViewModel()

        try {
            state.prefetchUpcomingArtwork(currentTrackId = 1, controls, artwork)

            val deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(5)
            while (
                tracks.any { track ->
                    val visual = cache.artwork(prefetchRequest(track))
                    visual == null || cache.fog(visual.image) == null
                } &&
                System.nanoTime() < deadline
            ) {
                Thread.yield()
            }
            tracks.forEach { track ->
                val visual = cache.artwork(prefetchRequest(track))
                assertNotNull(visual)
                assertNotNull(cache.fog(checkNotNull(visual).image))
            }
        } finally {
            artwork.shutdown()
        }
    }

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
    fun openingSearchStaysOnTheCurrentTabAndTheViewModelKeepsWhatItIsHanded() {
        val state = MobileSurfaceViewModel()

        state.selectTab(BrowseTab.ARTISTS)
        state.openSearch()
        state.updateSearch("slowdive")
        state.updateScroll(
            LibraryListKey.ARTISTS,
            LibraryScrollPosition(firstVisibleItemIndex = 18, itemOffsetFraction = 0.25f),
        )

        assertEquals(BrowseTab.ARTISTS, state.selectedTab)
        assertTrue(state.searchVisible)
        assertEquals("slowdive", state.searchText)
        assertEquals(
            LibraryScrollPosition(firstVisibleItemIndex = 18, itemOffsetFraction = 0.25f),
            state.scrollPosition(LibraryListKey.ARTISTS),
        )
    }

    @Test
    fun selectingQueueMovesThePagerWithoutOverwritingTheStoredLibraryDestination() {
        val remembered = mutableListOf<BrowseTab>()
        val state = MobileSurfaceViewModel()
        state.initializeSelectedTab(BrowseTab.TITLES, remembered::add)

        state.selectTab(BrowseTab.QUEUE)

        assertEquals(BrowseTab.QUEUE, state.selectedTab)
        assertEquals(emptyList<BrowseTab>(), remembered)

        state.selectTab(BrowseTab.ARTISTS)
        assertEquals(listOf(BrowseTab.ARTISTS), remembered)
    }

    @Test
    fun pagedInWindowsAreHandedBackOnlyWhileTheCatalogStillHasTheSameShape() {
        val state = MobileSurfaceViewModel()
        val paged = LoadedLibraryWindows(
            titles = LibraryWindow(total = 450, rows = emptyList(), hasMore = true),
            artists = LibraryWindow.empty(),
            openAlbum = null,
        )
        val shape = LibraryCatalogShape(titles = 450, artists = 0)

        state.keepLoadedWindows(shape, paged)

        assertEquals(paged, state.loadedWindows(shape))
        assertNull(state.loadedWindows(shape.copy(titles = 451)))
    }

    @Test
    fun pagedInWindowsAreRestoredOnlyWithTheSearchThatProducedThem() {
        val state = MobileSurfaceViewModel()
        val shape = LibraryCatalogShape(titles = 450, artists = 450)
        val filtered = LoadedLibraryWindows(
            titles = LibraryWindow.empty(),
            artists = LibraryWindow(total = 450, rows = emptyList(), hasMore = true),
            searchText = "slow",
            openAlbum = null,
        )

        state.updateSearch("slow")
        state.keepLoadedWindows(shape, filtered)

        assertEquals(filtered, state.loadedWindows(shape))
        state.updateSearch("")
        assertNull(state.loadedWindows(shape))
    }

    @Test
    fun artistAlbumRestoreDropsAnAlbumWhoseArtistIsMissing() {
        val state = MobileSurfaceViewModel()
        state.initializeSelectedTab(BrowseTab.ARTISTS) {}
        val shape = LibraryCatalogShape(titles = 0, artists = 1)
        val album = LibraryAlbum(
            title = "Hey What",
            artist = "Low",
            representativeUri = "content://hey-what",
            trackCount = 2,
            year = 2021,
            totalDurationMs = 120_000,
        )
        val restored = LoadedLibraryWindows(
            titles = LibraryWindow.empty(),
            artists = LibraryWindow.empty(),
            openAlbum = AlbumTrackList(album, LibraryWindow.empty()),
            openArtist = null,
        )

        state.keepLoadedWindows(shape, restored)

        assertNull(state.loadedWindows(shape)?.openAlbum)
    }

    @Test
    fun artistAlbumRestoreKeepsTheAlbumWhenItsArtistPageIsPresent() {
        val state = MobileSurfaceViewModel()
        state.initializeSelectedTab(BrowseTab.ARTISTS) {}
        val shape = LibraryCatalogShape(titles = 0, artists = 1)
        val album = AlbumTrackList(
            album = LibraryAlbum("Hey What", "Low", "content://hey-what", 2, 2021, 120_000),
            tracks = LibraryWindow.empty(),
        )
        val restored = LoadedLibraryWindows(
            titles = LibraryWindow.empty(),
            artists = LibraryWindow.empty(),
            openAlbum = album,
            openArtist = ArtistTrackList(
                artist = LibraryArtist("Low", 2, 1, "content://low"),
            ),
        )

        state.keepLoadedWindows(shape, restored)

        assertEquals(album, state.loadedWindows(shape)?.openAlbum)
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

        val released = checkNotNull(state.releaseScrub(trackId))
        assertEquals(48_000, released.positionMs)
        assertFalse(released.isDragging)
    }

    @Test
    fun landscapeOffersDockWithoutEnteringItAndPortraitIsAnExit() {
        val state = MobileSurfaceViewModel()

        state.observeSurfaceLayout(SurfaceLayout.STACKED)
        state.observeSurfaceLayout(SurfaceLayout.WIDE_SHORT)

        assertTrue(state.dockOfferVisible)
        assertFalse(state.dockMode)
        state.enterDockMode()
        assertTrue(state.dockMode)
        assertTrue(state.nowPlayingExpanded)

        state.observeSurfaceLayout(SurfaceLayout.STACKED)
        assertFalse(state.dockMode)
    }
}

private fun prefetchTrack(id: Long) = LibraryTrack(
    id = id,
    uri = "content://tracks/$id",
    title = "Track $id",
    artist = "Artist",
    album = "Album",
    durationMs = 180_000,
    playCount = 0,
    rating = 0,
)

private fun prefetchRequest(track: LibraryTrack) = ArtworkRequest(
    track.uri,
    AndroidArtworkSize.NOW_PLAYING,
    track.title,
    track.artist,
)
