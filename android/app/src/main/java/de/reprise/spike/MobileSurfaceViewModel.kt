package de.reprise.spike

import androidx.compose.material3.windowsizeclass.WindowHeightSizeClass
import androidx.compose.material3.windowsizeclass.WindowSizeClass
import androidx.compose.material3.windowsizeclass.WindowWidthSizeClass
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
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
    FAVOURITES,
    ALBUM_TRACKS,
    ARTIST_TRACKS,
    UPCOMING,
}

/**
 * The catalog windows a screen has actually paged in.
 *
 * These are here for the same reason the anchor is: the anchor is an *index*,
 * and an index into 400 paged-in rows means nothing to a replacement activity
 * that reloaded 200. It would not fail either — a lazy list quietly starts at
 * its last item — which is why keeping the rows and keeping the anchor is one
 * decision rather than two.
 *
 * The usual argument for leaving rows out of durable state is
 * `TransactionTooLargeException`, and that is a Binder limit on a parcel
 * crossing a process boundary. Nothing here crosses one: a ViewModel is an
 * object in this process, these rows are already in it, and the largest library
 * this app has been pointed at is a few hundred kilobytes of them.
 */
internal data class LoadedLibraryWindows(
    val titles: LibraryWindow<LibraryTrack>,
    val albums: LibraryWindow<LibraryAlbum>,
    val artists: LibraryWindow<LibraryArtist>,
    val favourites: LibraryWindow<LibraryTrack> = LibraryWindow.empty(),
    /**
     * The album the listener is standing in, if any, and the tracks of it they
     * have paged in. It is kept here rather than reopened, because reopening it
     * means asking the library for the album inside a composition — the read
     * that takes the lock a scan holds for its whole walk.
     */
    val openAlbum: AlbumTrackList?,
    val openArtist: ArtistTrackList? = null,
)

/**
 * What paged-in windows were counted against.
 *
 * A replacement activity reads the first window again. If the library no longer
 * holds the same number of titles, albums and artists, a scan changed it while
 * the screen was gone and the paged-in rows describe a catalog that is not
 * there any more; they are dropped rather than shown. Counts rather than the
 * rows themselves, because a play the listener just finished changes a row's
 * play count without changing what is in the library.
 */
internal data class LibraryCatalogShape(
    val titles: Long,
    val albums: Long,
    val artists: Long,
)

internal fun LibraryScreenState.Browse.catalogShape() = LibraryCatalogShape(
    titles = titles.total,
    albums = albums.total,
    artists = artists.total,
)

/**
 * State whose lifetime is the screen rather than one composition.
 *
 * Configuration changes replace the activity and every composition below it,
 * so the selected browser place — including an album that is standing open —
 * an open refinement, list anchors, the catalog windows those anchors point
 * into, overlays, and an in-flight scrub live here.
 * Transient errors, menus, and drawing state remain with the composition. The
 * playing track is deliberately absent: the playback session owns it and the
 * activity asks.
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
    var nowPlayingQueueVisible by mutableStateOf(false)
        private set
    var settingsVisible by mutableStateOf(false)
        private set
    var dockMode by mutableStateOf(false)
        private set
    var dockOfferVisible by mutableStateOf(false)
        private set
    var dockOfferVersion by mutableStateOf(0L)
        private set

    private val confirmedRatings = mutableStateMapOf<Long, Int>()
    private val scrollPositions = mutableMapOf<LibraryListKey, LibraryScrollPosition>()
    private var loadedWindows: LoadedLibraryWindows? = null
    private var loadedShape: LibraryCatalogShape? = null
    private var scrubTrackId: Long? = null
    private var scrubPosition by mutableStateOf<SeekPositionState?>(null)
    private var previousSurfaceLayout: SurfaceLayout? = null

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
        if (!show) nowPlayingQueueVisible = false
    }

    fun showNowPlayingQueue(show: Boolean) {
        nowPlayingQueueVisible = show
    }

    fun showSettings(show: Boolean) {
        settingsVisible = show
    }

    fun observeSurfaceLayout(layout: SurfaceLayout) {
        if (
            previousSurfaceLayout == SurfaceLayout.STACKED &&
            layout == SurfaceLayout.WIDE_SHORT &&
            !dockMode
        ) {
            dockOfferVisible = true
            dockOfferVersion += 1
        }
        previousSurfaceLayout = layout
        if (layout == SurfaceLayout.STACKED && dockMode) {
            exitDockMode()
        }
    }

    fun dismissDockOffer(version: Long = dockOfferVersion) {
        if (version == dockOfferVersion) dockOfferVisible = false
    }

    fun enterDockMode() {
        dockMode = true
        dockOfferVisible = false
        nowPlayingExpanded = true
    }

    fun exitDockMode() {
        dockMode = false
    }

    /**
     * The rating to show for a row: what the database last accepted for that
     * track while this screen has been alive, and otherwise what the row was
     * loaded with.
     *
     * Three surfaces show one track's favourite state — the library row, the
     * sheet and the dock — and each of them used to keep its own
     * `remember`ed copy that only its *own* successful write moved. Nothing
     * reloads a row after a rating write (the playing row is re-read when the
     * *track* changes, never when its rating does), so rating in the dock and
     * stepping back out through ✕ left the sheet showing the value from before
     * the visit. One place, one copy: the surfaces read this and remember
     * nothing themselves.
     *
     * It holds one entry per track rated during the screen's life, which is as
     * many as a listener taps — the map is never a copy of the library.
     */
    fun ratingOf(track: LibraryTrack): Int =
        confirmedRatings[track.id] ?: track.rating.coerceIn(0, 5)

    /**
     * Takes up a rating, and only ever one the database has already agreed to.
     *
     * This is the sole writer of what [ratingOf] answers, which is what keeps
     * the heart from moving early: setting it optimistically and rolling back
     * would be a heart telling the listener something nobody had checked.
     */
    fun confirmFavourite(trackId: Long, favourite: Boolean) {
        confirmedRatings[trackId] = if (favourite) 5 else 0
    }

    fun scrollPosition(list: LibraryListKey): LibraryScrollPosition =
        scrollPositions[list] ?: LibraryScrollPosition()

    fun updateScroll(list: LibraryListKey, position: LibraryScrollPosition) {
        scrollPositions[list] = position
    }

    /** The paged-in windows, while they still belong to the catalog on disk. */
    fun loadedWindows(shape: LibraryCatalogShape): LoadedLibraryWindows? =
        loadedWindows.takeIf { loadedShape == shape }

    fun keepLoadedWindows(shape: LibraryCatalogShape, windows: LoadedLibraryWindows) {
        loadedShape = shape
        loadedWindows = windows
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
