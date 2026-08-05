package de.reprise.spike

import androidx.compose.material3.windowsizeclass.WindowHeightSizeClass
import androidx.compose.material3.windowsizeclass.WindowSizeClass
import androidx.compose.material3.windowsizeclass.WindowWidthSizeClass
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.lifecycle.ViewModel

/** The two surface arrangements M9a supports; neither is an orientation. */
internal enum class SurfaceLayout {
    STACKED,
    WIDE_SHORT,
}

internal fun surfaceLayoutFor(windowSizeClass: WindowSizeClass): SurfaceLayout =
    if (
        windowSizeClass.widthSizeClass == WindowWidthSizeClass.Expanded &&
        windowSizeClass.heightSizeClass == WindowHeightSizeClass.Compact
    ) {
        SurfaceLayout.WIDE_SHORT
    } else {
        SurfaceLayout.STACKED
    }

internal enum class LibraryListKey {
    TITLES,
    ALBUMS,
    ARTISTS,
    ALBUM_TRACKS,
}

internal data class LibraryScrollPosition(
    val firstVisibleItemIndex: Int = 0,
    val firstVisibleItemScrollOffset: Int = 0,
)

/**
 * State whose lifetime is the screen rather than one composition.
 *
 * Configuration changes replace the activity and every composition below it,
 * so the selected browser place, an open refinement, list anchors, overlays,
 * and an in-flight scrub live here. Loaded catalog windows, transient errors,
 * menus, and drawing state remain with the composition. The playing track is
 * deliberately absent: the playback session owns it and the activity asks.
 */
internal class MobileSurfaceViewModel : ViewModel() {
    var selectedTab by mutableStateOf(BrowseTab.TITLES)
        private set
    var searchVisible by mutableStateOf(false)
        private set
    var searchText by mutableStateOf("")
        private set
    var nowPlayingExpanded by mutableStateOf(false)
        private set
    var settingsVisible by mutableStateOf(false)
        private set

    private val scrollPositions = mutableMapOf<LibraryListKey, LibraryScrollPosition>()
    private var scrubTrackId: Long? = null
    private var scrubPosition by mutableStateOf<SeekPositionState?>(null)

    fun selectTab(tab: BrowseTab) {
        selectedTab = tab
        if (tab != BrowseTab.TITLES) {
            searchVisible = false
        }
    }

    fun openSearch() {
        selectedTab = BrowseTab.TITLES
        searchVisible = true
    }

    fun closeSearch() {
        searchVisible = false
    }

    fun updateSearch(text: String) {
        searchText = text
    }

    fun showNowPlaying(show: Boolean) {
        nowPlayingExpanded = show
    }

    fun showSettings(show: Boolean) {
        settingsVisible = show
    }

    fun scrollPosition(list: LibraryListKey): LibraryScrollPosition =
        scrollPositions[list] ?: LibraryScrollPosition()

    fun updateScroll(list: LibraryListKey, position: LibraryScrollPosition) {
        scrollPositions[list] = position
    }

    fun seekPosition(trackId: Long, fallbackPositionMs: Long): SeekPositionState =
        scrubPosition.takeIf { scrubTrackId == trackId }
            ?: SeekPositionState.fromSnapshot(fallbackPositionMs)

    fun acceptPlaybackSnapshot(trackId: Long, positionMs: Long) {
        val current = seekPosition(trackId, positionMs)
        scrubTrackId = trackId
        scrubPosition = current.acceptSnapshot(positionMs)
    }

    fun dragTo(trackId: Long, positionMs: Long) {
        scrubTrackId = trackId
        scrubPosition = seekPosition(trackId, positionMs).dragTo(positionMs)
    }

    fun releaseScrub(trackId: Long): SeekPositionState {
        val released = seekPosition(trackId, fallbackPositionMs = 0).release()
        scrubTrackId = trackId
        scrubPosition = released
        return released
    }
}
