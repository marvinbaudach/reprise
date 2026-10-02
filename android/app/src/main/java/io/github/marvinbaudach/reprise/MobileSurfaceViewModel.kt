package io.github.marvinbaudach.reprise

import android.content.Context
import android.net.ConnectivityManager
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import androidx.compose.material3.windowsizeclass.WindowHeightSizeClass
import androidx.compose.material3.windowsizeclass.WindowSizeClass
import androidx.compose.material3.windowsizeclass.WindowWidthSizeClass
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.lifecycle.ViewModel
import uniffi.reprise_android_ffi.MusicLibrary

private const val ANALYSIS_PREFETCH_OFFSET = -3L
private const val ANALYSIS_PREFETCH_LIMIT = 5L
private const val ALBUM_COVER_REFRESH_WINDOW_MS = 2_000L

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
    ARTISTS,
    ALBUM_TRACKS,
    ARTIST_ALBUMS,
    ARTIST_TRACKS,
    UPCOMING,
}

/**
 * What a saved scroll anchor is keyed on.
 *
 * [LibraryListKey] alone names a *kind* of list, not one — every album opens
 * the same [LibraryListKey.ALBUM_TRACKS] list, and every artist page the same
 * [LibraryListKey.ARTIST_ALBUMS] one. [owner] narrows that to the one album or
 * artist the anchor actually belongs to, so leaving album A at row 30 and
 * opening album B does not hand B row 30 back. It stays empty for the lists
 * that only ever have one instance (titles, artists, the queue), which keeps
 * their anchors behaving exactly as before.
 */
internal data class LibraryListIdentity(val list: LibraryListKey, val owner: String = "")

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
    val artists: LibraryWindow<LibraryArtist>,
    val loadedTabs: Set<BrowseTab> = BrowseTab.entries.toSet(),
    val searchText: String = "",
    /**
     * The album the listener is standing in, if any, and the tracks of it they
     * have paged in. It is kept here rather than reopened, because reopening it
     * means asking the library for the album inside a composition — synchronous
     * database I/O that belongs off the main thread.
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
    val artists: Long,
)

internal fun LibraryScreenState.Browse.catalogShape() = LibraryCatalogShape(
    titles = titles.total,
    artists = artists.total,
)

private data class ArtistPhotoBackfillBinding(
    val snapshot: () -> ArtistPhotoProgress,
    val start: ((ArtistPhotoProgress) -> Unit) -> Unit,
    val cancel: () -> Unit,
    val postToMain: (() -> Unit) -> Unit,
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
internal class MobileSurfaceViewModel(
    private val nowMillis: () -> Long = SystemClock::elapsedRealtime,
    private val scheduleAfter: (Long, () -> Unit) -> Unit = { delayMs, work ->
        Handler(Looper.getMainLooper()).postDelayed(work, delayMs)
    },
) : ViewModel(), DeletionMessages {
    val networkReturnDetector = NetworkReturnDetector()
    private var networkReturnMonitor: NetworkReturnMonitor? = null

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
    var dockMode by mutableStateOf(false)
        private set
    var dockOfferVisible by mutableStateOf(false)
        private set
    var dockOfferVersion by mutableStateOf(0L)
        private set
    /**
     * What a running deletion is doing, and what the last one did. Here rather
     * than on the row the deletion started from: that row, its page, even its
     * list may be gone by the time the deletion answers.
     */
    private var runningDeletions by mutableStateOf(emptyList<DeletionProgress>())
    private var lastDeletionRun = 0L

    /** The latest deletion still waiting for its answer, if any. */
    val deletionProgress: DeletionProgress?
        get() = runningDeletions.lastOrNull()
    var deletionMessage by mutableStateOf<TransientMessage?>(null)
        private set
    private var refreshTickets = 0
    private var appliedRefreshTicket = 0
    private var artistPhotoProgress by mutableStateOf<ArtistPhotoProgress?>(null)
    private var dismissedArtistPhotoRunId by mutableStateOf<Long?>(null)
    private var refreshArtistPortraits: () -> Unit = {}
    private var refreshedArtistPortraitRunId = 0L
    private var refreshedArtistPortraitDone = 0L
    private var refreshAlbumCovers: () -> Unit = {}
    private var refreshedAlbumCoverRunId = 0L
    private var observedAlbumCoverDone = 0L
    private var refreshedAlbumCoverDone = 0L
    private var albumCoverWindowStartedAtMs: Long? = null
    private var albumCoverScheduleGeneration = 0L
    @Volatile
    private var artistPhotoBackfillBinding: ArtistPhotoBackfillBinding? = null
    val libraryScanMonitor = Any()
    private var reportLibraryState: ((LibraryScreenState) -> Unit)? = null
    private var pendingLibraryState: LibraryScreenState? = null

    val visibleArtistPhotoProgress: ArtistPhotoProgress?
        get() = artistPhotoProgress?.takeIf { update ->
            update.runId != 0L &&
                update.runId != dismissedArtistPhotoRunId &&
                !(update.phase == ArtistPhotoProgressPhase.COMPLETE && update.total == 0L)
        }

    private val confirmedRatings = mutableStateMapOf<Long, Int>()
    private val scrollPositions = mutableMapOf<LibraryListIdentity, LibraryScrollPosition>()
    private var loadedWindows: LoadedLibraryWindows? = null
    private var loadedShape: LibraryCatalogShape? = null
    private var scrubTrackId: Long? = null
    private var scrubPosition by mutableStateOf<SeekPositionState?>(null)
    private var previousSurfaceLayout: SurfaceLayout? = null
    private var selectedTabInitialized = false
    private var rememberSelectedTab: (BrowseTab) -> Unit = {}
    private var prefetchedForTrackId: Long? = null

    fun bindLibraryStateReporter(report: (LibraryScreenState) -> Unit): () -> Unit {
        reportLibraryState = report
        pendingLibraryState?.let(report)
        pendingLibraryState = null
        return {
            if (reportLibraryState === report) reportLibraryState = null
        }
    }

    fun updateLibraryState(state: LibraryScreenState) {
        reportLibraryState?.invoke(state) ?: run { pendingLibraryState = state }
    }

    /**
     * Orders refreshes across activity recreation: this outlives the activity
     * that started one, so a refresh from before a rotation cannot land after
     * one from after it. Main thread only.
     */
    fun takeRefreshTicket(): Int = ++refreshTickets

    fun isNewestRefresh(ticket: Int): Boolean = ticket > appliedRefreshTicket

    /** What the screen shows right now, taken on the main thread before a refresh reads. */
    fun removalRefreshBasis() = RemovalRefreshBasis(loadedWindows, selectedTab)

    /**
     * Hands over the library as it is after a deletion.
     *
     * The rebuilt windows are stored under the new catalog's shape first, so the
     * screen's own restore path takes them up — open pages, paged-in rows and
     * refinement together — instead of falling back to the first 200 rows. Both
     * calls happen in one main-thread turn: no composition can run between them.
     * They are only stored while [basis] still describes the screen; a listener
     * who moved on meanwhile gets the plain fresh state, as after a scan.
     */
    fun updateLibraryAfterRemoval(
        refreshed: RefreshedLibrary,
        basis: RemovalRefreshBasis,
        ticket: Int,
    ) {
        appliedRefreshTicket = maxOf(appliedRefreshTicket, ticket)
        val windows = refreshed.windows
        if (windows != null && basis.stillDescribes(loadedWindows, selectedTab, searchText)) {
            keepLoadedWindows(refreshed.state.catalogShape(), windows)
        }
        updateLibraryState(refreshed.state)
    }

    override fun say(text: String) {
        deletionMessage = TransientMessage(text).after(deletionMessage)
    }

    override fun begin(text: String): DeletionRun {
        val started = DeletionProgress(run = ++lastDeletionRun, text = text)
        runningDeletions = runningDeletions + started
        return DeletionRun { outcome ->
            runningDeletions = runningDeletions.filterNot { it.run == started.run }
            say(outcome)
        }
    }

    fun dismissDeletionMessage() {
        deletionMessage = null
    }

    fun bindArtistPhotoBackfill(
        snapshot: () -> ArtistPhotoProgress,
        start: ((ArtistPhotoProgress) -> Unit) -> Unit,
        cancel: () -> Unit,
        postToMain: (() -> Unit) -> Unit = { work -> work() },
    ) {
        artistPhotoBackfillBinding = ArtistPhotoBackfillBinding(
            snapshot = snapshot,
            start = start,
            cancel = cancel,
            postToMain = postToMain,
        )
        val initial = snapshot()
        postToMain { acceptArtistPhotoProgress(initial) }
    }

    fun bindArtistPortraitRefresh(refresh: () -> Unit) {
        refreshArtistPortraits = refresh
    }

    fun bindAlbumCoverRefresh(refresh: () -> Unit) {
        refreshAlbumCovers = refresh
    }

    fun startNetworkReturnMonitor(context: Context, onNetworkReturned: () -> Unit) {
        stopNetworkReturnMonitor()
        val connectivity = context.applicationContext
            .getSystemService(ConnectivityManager::class.java)
        networkReturnMonitor = NetworkReturnMonitor(
            connectivity = connectivity,
            detector = networkReturnDetector,
            onNetworkReturned = onNetworkReturned,
        ).also(NetworkReturnMonitor::start)
    }

    fun stopNetworkReturnMonitor() {
        networkReturnMonitor?.stop()
        networkReturnMonitor = null
    }

    fun startArtistPhotoBackfill() {
        val binding = artistPhotoBackfillBinding ?: return
        binding.start { update ->
            binding.postToMain { acceptArtistPhotoProgress(update) }
        }
        val snapshot = binding.snapshot()
        binding.postToMain { acceptArtistPhotoProgress(snapshot) }
    }

    fun acceptArtistPhotoProgress(update: ArtistPhotoProgress) {
        if (update.runId != refreshedArtistPortraitRunId) {
            refreshedArtistPortraitRunId = update.runId
            refreshedArtistPortraitDone = 0
        }
        val portraitDone = update.done - update.coversDone
        if (portraitDone > refreshedArtistPortraitDone) {
            refreshArtistPortraits()
            refreshedArtistPortraitDone = portraitDone
        }
        acceptAlbumCoverProgress(update)
        artistPhotoProgress = update
    }

    private fun acceptAlbumCoverProgress(update: ArtistPhotoProgress) {
        if (update.runId != refreshedAlbumCoverRunId) {
            flushAlbumCoverRefresh()
            refreshedAlbumCoverRunId = update.runId
            observedAlbumCoverDone = 0
            refreshedAlbumCoverDone = 0
            albumCoverWindowStartedAtMs = null
            albumCoverScheduleGeneration++
        }
        if (update.coversDone > observedAlbumCoverDone) {
            observedAlbumCoverDone = update.coversDone
            val now = nowMillis()
            val windowStartedAt = albumCoverWindowStartedAtMs
            if (
                windowStartedAt != null &&
                now - windowStartedAt >= ALBUM_COVER_REFRESH_WINDOW_MS
            ) {
                flushAlbumCoverRefresh()
            } else if (windowStartedAt == null) {
                albumCoverWindowStartedAtMs = now
                val scheduledGeneration = ++albumCoverScheduleGeneration
                scheduleAfter(ALBUM_COVER_REFRESH_WINDOW_MS) {
                    if (scheduledGeneration == albumCoverScheduleGeneration) {
                        flushAlbumCoverRefresh()
                    }
                }
            }
        }
        if (observedAlbumCoverDone <= refreshedAlbumCoverDone) return

        val complete = observedAlbumCoverDone > 0 && (
            update.phase == ArtistPhotoProgressPhase.COMPLETE ||
                (update.coversTotal > 0 && observedAlbumCoverDone >= update.coversTotal)
        )
        if (!complete) return

        flushAlbumCoverRefresh()
    }

    private fun flushAlbumCoverRefresh() {
        if (observedAlbumCoverDone <= refreshedAlbumCoverDone) return
        refreshAlbumCovers()
        refreshedAlbumCoverDone = observedAlbumCoverDone
        albumCoverWindowStartedAtMs = null
        albumCoverScheduleGeneration++
    }

    fun dismissArtistPhotoProgress() {
        dismissedArtistPhotoRunId = artistPhotoProgress?.runId
    }

    fun cancelArtistPhotoBackfill() {
        artistPhotoBackfillBinding?.cancel?.invoke()
        dismissArtistPhotoProgress()
    }

    fun initializeSelectedTab(initial: BrowseTab, remember: (BrowseTab) -> Unit) {
        rememberSelectedTab = remember
        if (selectedTabInitialized) return
        selectedTab = initial
        selectedTabInitialized = true
    }

    fun selectTab(tab: BrowseTab) {
        if (tab == selectedTab) return
        selectedTab = tab
        if (tab != BrowseTab.QUEUE) rememberSelectedTab(tab)
        // A standing search follows the listener from tab to tab and filters
        // the one they land on — closing it on the way out was the whole
        // complaint. The queue is the exception: it is the one tab no filter
        // reaches, so a field left open there would promise something it
        // cannot do.
        if (tab == BrowseTab.QUEUE) {
            searchVisible = false
        }
    }

    fun openSearch() {
        // The queue is the one tab a filter never reaches (see selectTab
        // above), so opening the field here would collect text that then
        // silently seeds whichever tab the listener swipes to next.
        if (selectedTab == BrowseTab.QUEUE) return
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

    /** Warms the current track and its two queue neighbours on either side once. */
    fun prefetchUpcomingArtwork(
        currentTrackId: Long?,
        controls: PlaybackControls,
        artwork: TrackArtwork?,
        analysis: TrackAnalysisPort? = TrackAnalysisLoader.activePort(),
    ) {
        if (currentTrackId == null) {
            prefetchedForTrackId = null
            analysis?.retain(emptySet())
            return
        }
        if (artwork == null && analysis == null) {
            prefetchedForTrackId = null
            return
        }
        // This guard records the whole current-track tenure, not which ports were present.
        // Analysis is therefore expected on the first call; it is not a LaunchedEffect key,
        // so analysis becoming non-null later does not prefetch again for the same track.
        if (prefetchedForTrackId == currentTrackId) return
        prefetchedForTrackId = currentTrackId
        controls.loadUpcomingTracks(
            LibraryWindowRange(ANALYSIS_PREFETCH_OFFSET, ANALYSIS_PREFETCH_LIMIT),
        ) { outcome ->
            if (prefetchedForTrackId != currentTrackId) return@loadUpcomingTracks
            val tracks = outcome.getOrNull()?.rows.orEmpty()
            val trackIds = tracks.map { track -> track.id }
            analysis?.retain(trackIds.toSet())
            analysis?.prefetch(trackIds)
            tracks.filterNot { track -> track.id == currentTrackId }.forEach { track ->
                artwork?.prefetch(
                    ArtworkRequest(
                        trackUri = track.uri,
                        // The full-size shelf holds the three rendered panels. Keep the
                        // wider prefetch window in the list shelf; seedVisual reads it
                        // synchronously across sizes while the panel resolves full size.
                        size = uniffi.reprise_android_ffi.AndroidArtworkSize.LIST,
                        title = track.title,
                        artist = track.artist,
                    ),
                )
            }
        }
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

    fun scrollPosition(list: LibraryListKey, owner: String = ""): LibraryScrollPosition =
        scrollPositions[LibraryListIdentity(list, owner)] ?: LibraryScrollPosition()

    fun updateScroll(list: LibraryListKey, position: LibraryScrollPosition, owner: String = "") {
        scrollPositions[LibraryListIdentity(list, owner)] = position
    }

    /** Paged-in windows while both their catalog and refinement still match. */
    fun loadedWindows(shape: LibraryCatalogShape): LoadedLibraryWindows? =
        loadedWindows
            ?.takeIf { loadedShape == shape && it.searchText == searchText }
            ?.let { windows ->
                if (
                    selectedTab == BrowseTab.ARTISTS &&
                    windows.openAlbum != null &&
                    windows.openArtist == null
                ) {
                    windows.copy(openAlbum = null)
                } else {
                    windows
                }
            }

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

    fun releaseScrub(trackId: Long): SeekPositionState? {
        if (scrubTrackId != trackId) return null
        val released = checkNotNull(scrubPosition).release()
        scrubTrackId = trackId
        scrubPosition = released
        return released
    }

    fun cancelScrub(trackId: Long) {
        if (scrubTrackId != trackId) return
        scrubPosition = checkNotNull(scrubPosition).cancel()
    }

    override fun onCleared() {
        stopNetworkReturnMonitor()
        albumCoverScheduleGeneration++
        refreshAlbumCovers = {}
        refreshArtistPortraits = {}
        val backfill = artistPhotoBackfillBinding
        artistPhotoBackfillBinding = null
        backfill?.cancel?.invoke()
    }
}
