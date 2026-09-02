package io.github.marvinbaudach.reprise

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.expandVertically
import androidx.compose.animation.shrinkVertically
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.animation.core.MutableTransitionState
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.asPaddingValues
import androidx.compose.foundation.layout.calculateStartPadding
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.pager.HorizontalPager
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.NavigationRailDefaults
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.snapshotFlow
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.lifecycle.viewmodel.compose.viewModel
import io.github.marvinbaudach.reprise.settings.SettingsNavigation
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.withContext

/**
 * One answered request for the playing track's row, carrying the id it was
 * asked for. The id says whether the retained row still describes the track
 * the session reports as playing, so actions can be disabled during a change.
 */
/**
 * How long the screen has to have been still before a tab nobody is looking at
 * is fetched. Long enough for the opening composition to be done with the main
 * thread, short enough to be over before a first swipe can plausibly land.
 */
internal const val NEIGHBOUR_PREFETCH_IDLE_MS = 400L

private data class AnsweredTrack(val id: Long, val track: LibraryTrack?)

/**
 * One tab's freshly fetched windows, carried out of the IO dispatcher.
 *
 * A tab fills a different set of windows than its neighbours, and a window it
 * does not fill is `null` rather than empty: an empty window is a real answer
 * — "no artists match this" — and assigning one where nothing was asked for
 * would blank a list that had rows.
 */
private data class LoadedTab(
    val titles: LibraryWindow<LibraryTrack>? = null,
    val artists: LibraryWindow<LibraryArtist>? = null,
)

internal enum class BrowseTab(val label: String, val symbol: String) {
    TITLES("Titles", "library_music"),
    ARTISTS("Artists", "artist"),
    QUEUE("Queue", "queue_music"),
}

/**
 * The library screen: which tab is showing, what each one has loaded so far,
 * which selection is playing, and whether Now Playing is expanded.
 *
 * Transport commands are deliberately absent from the parameter list. They are
 * used by the mini player and by [NowPlayingSheet], neither of which this
 * function is; they arrive through [LocalPlaybackControls] instead, the same
 * way covers arrive through [LocalTrackArtwork].
 */
@Composable
internal fun BrowseScreen(
    state: LibraryScreenState.Browse,
    playback: LibraryPlayback,
    playbackProgress: () -> Float = { 0f },
    nowPlayingPlayback: () -> PlaybackUiState = { PlaybackUiState() },
    playbackSettingsRevision: Long,
    surfaceLayout: SurfaceLayout = SurfaceLayout.STACKED,
    surfaceState: MobileSurfaceViewModel = viewModel(),
    chooseFolder: () -> Unit,
    rescan: () -> Unit,
    searchTitles: (String, LibraryWindowRange) -> LibraryWindow<LibraryTrack>,
    listArtists: (LibraryWindowRange) -> LibraryWindow<LibraryArtist>,
    searchArtists: (String, LibraryWindowRange) -> LibraryWindow<LibraryArtist> =
        { _, range -> listArtists(range) },
    openAlbum: (LibraryAlbum) -> AlbumTrackList,
    listAlbumTracks: (LibraryAlbum, LibraryWindowRange) -> LibraryWindow<LibraryTrack>,
    openArtist: (LibraryArtist) -> ArtistTrackList = { artist ->
        ArtistTrackList(artist = artist)
    },
    listArtistTracks: (LibraryArtist, LibraryWindowRange) -> LibraryWindow<LibraryTrack> =
        { _, _ -> LibraryWindow.empty() },
    listArtistAlbums: (LibraryArtist, LibraryWindowRange) -> LibraryWindow<LibraryAlbum> =
        { _, _ -> LibraryWindow.empty() },
    listArtistUntaggedTracks:
        (LibraryArtist, LibraryWindowRange) -> LibraryWindow<LibraryTrack> =
        { _, _ -> LibraryWindow.empty() },
    loadTrack: (Long, (LibraryTrack?) -> Unit) -> Unit,
    playTracks: (PlaybackSelection, (String) -> Unit) -> Unit,
    loadPlaybackSettings: () -> PlaybackSettingsUiState,
    setEqualizerEnabled: (Boolean) -> PlaybackSettingsUiState,
    replaceEqualizerCurve: (List<EqualizerCurvePoint>) -> PlaybackSettingsUiState,
    setGaplessEnabled: (Boolean) -> PlaybackSettingsUiState,
    themeSelection: MobileThemeSelection,
    selectTheme: (MobileTheme) -> Unit,
    onlineSourcesEnabled: Boolean = false,
    setOnlineSourcesEnabled: (Boolean) -> Unit = {},
    artistPhotoOfferSettled: Boolean = true,
    downloadArtistPhotos: () -> Unit = {},
    declineArtistPhotos: () -> Unit = {},
) {
    val trackAnalysis = LocalTrackAnalysis.current
    val playbackControls = LocalPlaybackControls.current
    val trackArtwork = LocalTrackArtwork.current
    val selectedTab = surfaceState.selectedTab
    val searchVisible = surfaceState.searchVisible
    val searchText = surfaceState.searchText
    LaunchedEffect(surfaceLayout) { surfaceState.observeSurfaceLayout(surfaceLayout) }
    LaunchedEffect(surfaceState.dockOfferVersion, surfaceState.dockOfferVisible) {
        if (surfaceState.dockOfferVisible) {
            val version = surfaceState.dockOfferVersion
            delay(4_000)
            surfaceState.dismissDockOffer(version)
        }
    }
    // Everything the listener has paged in, or the first window when there is
    // nothing to take up: a replacement activity reloads one window, and the
    // anchors kept above are indices into all of them.
    val shape = state.catalogShape()
    val restored = remember(state) { surfaceState.loadedWindows(shape) }
    var visibleTitles by remember(state) { mutableStateOf(restored?.titles ?: state.titles) }
    var visibleArtists by remember(state) { mutableStateOf(restored?.artists ?: state.artists) }
    var loadedTabs by remember(state) {
        mutableStateOf(restored?.loadedTabs ?: state.loadedTabs)
    }
    var selectedAlbum by remember(state) { mutableStateOf(restored?.openAlbum) }
    var selectedArtist by remember(state) { mutableStateOf(restored?.openArtist) }
    var browseError by remember(state) { mutableStateOf(state.message) }
    var visibleLoadRetryRevision by remember(state, searchText, selectedTab) {
        mutableIntStateOf(0)
    }
    var titlesRequestedOffset by remember(state, searchText) { mutableStateOf<Long?>(null) }
    var artistsRequestedOffset by remember(state, searchText) { mutableStateOf<Long?>(null) }
    var albumRequestedOffset by remember(state, selectedAlbum?.album) { mutableStateOf<Long?>(null) }
    var artistRequestedOffset by remember(state, selectedArtist?.artist) {
        mutableStateOf<Long?>(null)
    }
    var artistAlbumsRequestedOffset by remember(state, selectedArtist?.artist) {
        mutableStateOf<Long?>(null)
    }
    val nowPlayingExpanded = surfaceState.nowPlayingExpanded
    val settingsVisible = surfaceState.settingsVisible
    var settingsState by remember { mutableStateOf<PlaybackSettingsUiState?>(null) }
    val pagerState = rememberPagerState(
        initialPage = selectedTab.ordinal,
        pageCount = { BrowseTab.entries.size },
    )
    // What the bar marks and what the header counts is the page the gesture has
    // already committed to — not the one it settled on. `settledPage`, which the
    // state below is driven from, holds its old value for the whole drag *and*
    // the whole fling, so a bar reading it cannot move until everything has come
    // to rest: the pill sits under the tab being left while the tab being
    // entered is already filling the screen. `targetPage` turns over the moment
    // a swipe passes the point of no return, and at once on a tap.
    //
    // Handed on as a function, never as a value. Read here it would put a
    // mid-swipe invalidation on this whole composable; read where it is
    // rendered it invalidates the pill and the count line alone.
    val shownTab: () -> BrowseTab = remember(pagerState) {
        { BrowseTab.entries[pagerState.targetPage] }
    }

    fun selectDestination(tab: BrowseTab) {
        if (tab != selectedTab) {
            selectedAlbum = null
            selectedArtist = null
        }
        surfaceState.showNowPlaying(false)
        surfaceState.selectTab(tab)
    }

    LaunchedEffect(selectedTab) {
        if (pagerState.currentPage != selectedTab.ordinal) {
            pagerState.animateScrollToPage(selectedTab.ordinal)
        }
    }
    LaunchedEffect(pagerState) {
        snapshotFlow { pagerState.settledPage }
            .distinctUntilChanged()
            .drop(1)
            .collect { page -> selectDestination(BrowseTab.entries[page]) }
    }

    // A failure has to leave a *state* behind, never null: null renders
    // nothing at all, and there is no previous state to fall back on the first
    // time round — or after a rotation, which throws this one away and restores
    // `settingsVisible` without it.
    fun failedSettings(message: String): PlaybackSettingsUiState = settingsState?.copy(error = message)
        ?: PlaybackSettingsUiState(
            equalizerEnabled = false,
            gaplessEnabled = false,
            equalizerBands = emptyList(),
            error = message,
        )

    fun openSettings() {
        settingsState = runCatching(loadPlaybackSettings).getOrElse { error ->
            failedSettings("Could not load playback settings: ${error.message ?: "unknown error"}")
        }
        surfaceState.showSettings(true)
    }

    fun updateSettings(action: () -> PlaybackSettingsUiState) {
        settingsState = runCatching(action).getOrElse { error ->
            failedSettings("Could not save playback settings: ${error.message ?: "unknown error"}")
        }
    }

    // Also the reload after a rotation: this runs on entering the composition,
    // and `settingsVisible` is saveable while the settings themselves are not.
    LaunchedEffect(playbackSettingsRevision) {
        if (settingsVisible) {
            settingsState = runCatching(loadPlaybackSettings).getOrElse { error ->
                failedSettings(
                    "Could not refresh playback settings: ${error.message ?: "unknown error"}",
                )
            }
        }
    }

    fun play(selection: PlaybackSelection) {
        browseError = null
        playTracks(selection) { message -> browseError = message }
    }

    fun openAlbumDetail(album: LibraryAlbum) {
        runCatching { openAlbum(album) }
            .onSuccess { detail ->
                selectedAlbum = detail
                albumRequestedOffset = null
                browseError = null
            }
            .onFailure { error -> browseError = error.browseDetail("open the album") }
    }

    fun artistsFor(text: String, request: LibraryWindowRange) = if (text.isBlank()) {
        listArtists(request)
    } else {
        searchArtists(text, request)
    }

    fun search(text: String) {
        // A refinement is a question about a list, so it has to be answered on
        // the list. An open artist page — or the album page nested inside it —
        // would otherwise stay up and answer with that artist's *albums* where
        // the listener asked for artists. The field is shared across the tabs,
        // so this holds wherever the text was typed: the swipe back to Artists
        // lands on whatever is still open there. An empty query is exempt; that
        // is the field being closed again, not a question being asked.
        if (text.isNotBlank()) {
            selectedAlbum = null
            selectedArtist = null
        }
        // Only the tab this fills can be said to hold the refinement afterwards,
        // so the whole set goes first and exactly one comes back. Asking whether
        // the *text* changed would not be enough: a rescan re-enters here with
        // the same query while handing over freshly loaded — and unfiltered
        // — windows that claim to be loaded already.
        loadedTabs = emptySet()
        surfaceState.updateSearch(text)
        runCatching {
            when (selectedTab) {
                BrowseTab.TITLES -> searchTitles(text, firstLibraryWindow()).also { window ->
                    visibleTitles = window
                    titlesRequestedOffset = null
                }
                BrowseTab.ARTISTS -> {
                    visibleArtists = artistsFor(text, firstLibraryWindow())
                    artistsRequestedOffset = null
                }
                // The queue is an order, not a view of the library, so there is
                // nothing here for a filter to narrow. Selecting it closes the
                // field — see MobileSurfaceViewModel.selectTab.
                BrowseTab.QUEUE -> Unit
            }
        }.onSuccess {
            loadedTabs = setOf(selectedTab)
            browseError = null
        }
            .onFailure { error -> browseError = error.browseDetail("search") }
    }

    fun toggleSearch() {
        if (searchVisible) {
            surfaceState.closeSearch()
            if (searchText.isNotEmpty()) search("")
        } else {
            surfaceState.openSearch()
        }
    }

    // What is on screen is what a replacement activity has to be able to put
    // back. Handed over from the composition itself rather than from each
    // place that changes a window, so the two cannot drift apart.
    //
    // Assembled *here*, in this function's own scope, and not inside the effect
    // below: a state value read only from an inner lambda invalidates only that
    // lambda, and an effect in this scope would then keep handing back the
    // window it saw first — 200 rows, however many the listener had paged in.
    val loaded = LoadedLibraryWindows(
        titles = visibleTitles,
        artists = visibleArtists,
        loadedTabs = loadedTabs,
        searchText = searchText,
        openAlbum = selectedAlbum,
        openArtist = selectedArtist,
    )
    SideEffect { surfaceState.keepLoadedWindows(shape, loaded) }

    // The query is durable, and so is the window it produced — for as long as
    // that window is still the catalog's. When a scan has changed the library
    // underneath, there is nothing to take up and the refinement is asked for
    // again rather than replayed from rows that no longer describe it.
    LaunchedEffect(state) {
        if (searchText.isNotEmpty() && restored == null) {
            search(searchText)
        }
    }

    // The tab on screen first, then whatever is still unfetched behind it.
    //
    // Opening the library fills rows for the tab it opens on and no other:
    // `LibrarySession.browseState` hands the rest back through `withoutRows()`,
    // carrying a total but no rows. A swipe draws the next page as soon as it
    // begins and only settles afterwards, so a tab whose rows are still absent
    // is drawn *empty* for the length of the gesture and fills once it lands —
    // "0 of 65 artists loaded", then 65. Fetching the tab next door while the
    // pager stands still closes that gap before anyone swipes into it.
    //
    // Keyed on `loadedTabs`, so this re-enters after each fetch and works
    // through what is left one tab at a time rather than firing them at once.
    // A prefetch stays silent: it must not clear an error the visible tab is
    // still showing, nor raise one for a tab nobody has asked for. A failed
    // hidden prefetch remains outside `loadedTabs` and is fetched when selected.
    // A visible failure gets one immediate re-attempt; a second failure leaves
    // the error standing without restarting this effect again.
    val pendingTab = (listOf(selectedTab) + BrowseTab.entries)
        .firstOrNull { it != BrowseTab.QUEUE && it !in loadedTabs }
    // `selectedTab` is a key as well as a component of `pendingTab`: selecting a
    // tab whose prefetch is already waiting out the idle period leaves
    // `pendingTab` unchanged, and without the restart the wait would run on
    // under a tab someone is looking at.
    LaunchedEffect(pendingTab, selectedTab, state, searchText, visibleLoadRetryRevision) {
        if (pendingTab == null) return@LaunchedEffect
        val visible = pendingTab == selectedTab
        if (!visible) {
            // A prefetch exists to be invisible, so it waits for a moment when
            // nothing is on the line: not the opening frames, where it would
            // compete with the first composition, and not a gesture, where a
            // query landing mid-swipe trades an empty list for a dropped frame.
            delay(NEIGHBOUR_PREFETCH_IDLE_MS)
            snapshotFlow { pagerState.isScrollInProgress }.first { !it }
        }
        runCatching {
            // The rows come off a blocking JNI + SQLite call; only the handover
            // to Compose belongs on the main thread.
            withContext(Dispatchers.IO) {
                when (pendingTab) {
                    BrowseTab.TITLES -> LoadedTab(titles = searchTitles(searchText, firstLibraryWindow()))
                    BrowseTab.ARTISTS -> LoadedTab(
                        artists = artistsFor(searchText, firstLibraryWindow()),
                    )
                    BrowseTab.QUEUE -> LoadedTab()
                }
            }
        }.onSuccess { loaded ->
            loaded.titles?.let { visibleTitles = it }
            loaded.artists?.let { visibleArtists = it }
            loadedTabs = loadedTabs + pendingTab
            if (visible) browseError = null
        }.onFailure { error ->
            if (error is CancellationException) throw error
            if (visible) {
                browseError = error.browseDetail("load ${pendingTab.label.lowercase()}")
                if (visibleLoadRetryRevision == 0) visibleLoadRetryRevision = 1
            }
        }
    }

    fun loadMoreTitles(request: LibraryWindowRange) {
        if (visibleTitles.nextRequest(titlesRequestedOffset) != request) return
        titlesRequestedOffset = request.offset
        runCatching { searchTitles(searchText, request) }
            .onSuccess { continuation ->
                visibleTitles = visibleTitles.append(continuation)
                browseError = null
            }
            .onFailure { error -> browseError = error.browseDetail("load more titles") }
    }

    fun loadMoreArtists(request: LibraryWindowRange) {
        if (visibleArtists.nextRequest(artistsRequestedOffset) != request) return
        artistsRequestedOffset = request.offset
        runCatching { artistsFor(searchText, request) }
            .onSuccess { continuation ->
                visibleArtists = visibleArtists.append(continuation)
                browseError = null
            }
            .onFailure { error -> browseError = error.browseDetail("load more artists") }
    }

    fun loadMoreAlbumTracks(request: LibraryWindowRange) {
        val detail = selectedAlbum ?: return
        if (detail.tracks.nextRequest(albumRequestedOffset) != request) return
        albumRequestedOffset = request.offset
        runCatching { listAlbumTracks(detail.album, request) }
            .onSuccess { continuation ->
                selectedAlbum = detail.copy(tracks = detail.tracks.append(continuation))
                browseError = null
            }
            .onFailure { error -> browseError = error.browseDetail("load more album tracks") }
    }

    fun loadMoreArtistTracks(request: LibraryWindowRange) {
        val detail = selectedArtist ?: return
        if (detail.untaggedTracks.nextRequest(artistRequestedOffset) != request) return
        artistRequestedOffset = request.offset
        runCatching { listArtistUntaggedTracks(detail.artist, request) }
            .onSuccess { continuation ->
                selectedArtist = detail.copy(
                    untaggedTracks = detail.untaggedTracks.append(continuation),
                )
                browseError = null
            }
            .onFailure { error -> browseError = error.browseDetail("load more other titles") }
    }

    fun loadMoreArtistAlbums(request: LibraryWindowRange) {
        val detail = selectedArtist ?: return
        if (detail.albums.nextRequest(artistAlbumsRequestedOffset) != request) return
        artistAlbumsRequestedOffset = request.offset
        runCatching { listArtistAlbums(detail.artist, request) }
            .onSuccess { continuation ->
                selectedArtist = detail.copy(albums = detail.albums.append(continuation))
                browseError = null
            }
            .onFailure { error -> browseError = error.browseDetail("load more artist albums") }
    }

    BackHandler(
        enabled = !nowPlayingExpanded && !settingsVisible &&
            (selectedAlbum != null || selectedArtist != null),
    ) {
        when {
            selectedAlbum != null -> selectedAlbum = null
            selectedArtist != null -> selectedArtist = null
        }
    }

    // The row behind the mini player and the sheet is database I/O, so it is
    // asked for from an effect and answered later, never fetched inside the
    // composition. Reads no longer wait for a folder scan, but they still do
    // not belong on the main thread. See [TrackLoader].
    var answeredTrack by remember { mutableStateOf<AnsweredTrack?>(null) }
    val playingTrackId = playback.currentTrackId
    val latestPlayingTrackId by rememberUpdatedState(playingTrackId)
    LaunchedEffect(playingTrackId, playbackControls, trackArtwork) {
        surfaceState.prefetchUpcomingArtwork(playingTrackId, playbackControls, trackArtwork)
    }
    LaunchedEffect(playingTrackId, playback.currentTrackUri) {
        if (playingTrackId != null) {
            trackAnalysis.prepare(playingTrackId)
            loadTrack(playingTrackId) { track ->
                if (latestPlayingTrackId != null) {
                    answeredTrack = AnsweredTrack(playingTrackId, track)
                }
            }
        } else {
            answeredTrack = null
        }
    }
    // The last answered row stays in place while a new track is being read, but
    // its actions are disabled because it no longer answers for what is playing.
    // A stopped session still blanks immediately: no replacement answer is due.
    val lastAnsweredTrack = answeredTrack
    val shownTrack = if (playingTrackId == null) null else lastAnsweredTrack?.track
    val shownTrackIsStale = lastAnsweredTrack != null && lastAnsweredTrack.id != playingTrackId
    val nowPlayingSheetState = remember { MutableTransitionState(false) }
    nowPlayingSheetState.targetState =
        nowPlayingExpanded && playingTrackId != null && shownTrack != null
    val summary: () -> String = remember(
        shownTab,
        loadedTabs,
        selectedTab,
        visibleTitles,
        selectedAlbum,
        selectedArtist,
        visibleArtists,
    ) {
        {
            // The bar may already mark a tab whose window has not been fetched yet:
            // the fetch waits for the page to settle, and counting an unfetched
            // window prints a nought that reads as an answer — "0 of 65 artists"
            // where the truth is "not asked yet". Until the marked tab is loaded
            // the line keeps answering for the one that is.
            val counted = shownTab()
                .takeIf { it == BrowseTab.QUEUE || it in loadedTabs }
                ?: selectedTab
            when (counted) {
                BrowseTab.TITLES -> visibleTitles.visibleCountLabel("title", "titles")
                BrowseTab.ARTISTS -> selectedAlbum?.tracks
                    ?.visibleCountLabel("track", "tracks")
                    ?: selectedArtist?.let { detail ->
                        val albums = detail.albums.total
                        val otherTitles = detail.untaggedTracks.total
                        "$albums ${if (albums == 1L) "album" else "albums"} · " +
                            "$otherTitles ${if (otherTitles == 1L) "other title" else "other titles"}"
                    }
                    ?: visibleArtists.visibleCountLabel("artist", "artists")
                BrowseTab.QUEUE -> "Queue"
            }
        }
    }
    val frameMetrics = libraryFrameMetrics(surfaceLayout)
    val nowPlayingFrameModifier = when (surfaceLayout) {
        SurfaceLayout.STACKED -> Modifier
            .fillMaxSize()
        SurfaceLayout.WIDE_SHORT -> Modifier
            .fillMaxSize()
            .padding(
                start = frameMetrics.navigationRailWidthDp.dp +
                    NavigationRailDefaults.windowInsets
                        .asPaddingValues()
                        .calculateStartPadding(LocalLayoutDirection.current),
            )
    }
    Box(modifier = Modifier.fillMaxSize()) {
        val libraryScaffold: @Composable (Modifier) -> Unit = { frameModifier ->
            Scaffold(
                modifier = frameModifier,
                containerColor = MaterialTheme.colorScheme.background,
                bottomBar = {
                    LibraryBottomFrame(
                        surfaceLayout = surfaceLayout,
                        currentTrack = shownTrack,
                        playback = playback,
                        progress = playbackProgress,
                        shownTab = shownTab,
                        selectTab = ::selectDestination,
                        openNowPlaying = { surfaceState.showNowPlaying(true) },
                        nowPlayingExpanded = nowPlayingExpanded,
                    )
                },
            ) { contentPadding ->
                Column(
                    modifier = Modifier
                        .fillMaxSize()
                        .padding(contentPadding),
                ) {
                    if (searchVisible) {
                        LibrarySearchField(
                            tab = selectedTab,
                            searchText = searchText,
                            search = ::search,
                            close = ::toggleSearch,
                        )
                    }
                    LibrarySummaryActions(
                        tab = selectedTab,
                        summary = summary,
                        searching = searchVisible,
                        toggleSearch = ::toggleSearch,
                        rescan = rescan,
                        openSettings = ::openSettings,
                    )
                    // Re-readable state, not timed acknowledgements; see TransientMessage.
                    browseError?.let { BrowseErrorLine(it) }
                    playback.error?.let { BrowseErrorLine(it) }
                    if (
                        !surfaceState.dockMode &&
                        !nowPlayingSheetState.currentState &&
                        !nowPlayingSheetState.targetState
                    ) {
                        playback.faultNotice?.let { BrowseErrorLine(it.text) }
                    }
                    ArtistPhotoLibraryStatus(
                        offerVisible = shouldOfferArtistPhotos(
                            onlineSourcesEnabled, artistPhotoOfferSettled, state.artists.total,
                        ),
                        downloadArtistPhotos = downloadArtistPhotos,
                        declineArtistPhotos = declineArtistPhotos,
                        progress = surfaceState.visibleArtistPhotoProgress,
                        dismissProgress = surfaceState::dismissArtistPhotoProgress,
                    )
                    HorizontalPager(
                        state = pagerState,
                        modifier = Modifier
                            .weight(1f)
                            .testTag("library-destination-pager"),
                        key = { page -> BrowseTab.entries[page] },
                    ) { page ->
                        val tab = BrowseTab.entries[page]
                        Box(
                            modifier = Modifier
                                .fillMaxSize()
                                .testTag("library-page-${tab.name}"),
                        ) {
                            when (tab) {
                                BrowseTab.TITLES -> TitlesTab(
                                    surfaceLayout = surfaceLayout,
                                    surfaceState = surfaceState,
                                    tracks = visibleTitles,
                                    searchText = searchText,
                                    playback = playback,
                                    lastRequestedOffset = titlesRequestedOffset,
                                    play = { index ->
                                        play(PlaybackSelection(visibleTitles.rows, index))
                                    },
                                    loadMore = ::loadMoreTitles,
                                )
                                BrowseTab.ARTISTS -> ArtistsTab(
                                    surfaceLayout = surfaceLayout,
                                    surfaceState = surfaceState,
                                    artists = visibleArtists,
                                    searchText = searchText,
                                    selectedArtist = selectedArtist,
                                    selectedAlbum = selectedAlbum,
                                    playback = playback,
                                    openArtist = { artist ->
                                        runCatching { openArtist(artist) }
                                            .onSuccess { detail ->
                                                selectedArtist = detail
                                                artistRequestedOffset = null
                                                artistAlbumsRequestedOffset = null
                                                browseError = null
                                                surfaceState.closeSearch()
                                                if (searchText.isNotEmpty()) {
                                                    surfaceState.updateSearch("")
                                                    loadedTabs = emptySet()
                                                }
                                            }
                                            .onFailure { error ->
                                                browseError = error.browseDetail("open the artist")
                                            }
                                    },
                                    openAlbum = ::openAlbumDetail,
                                    closeArtist = {
                                        selectedAlbum = null
                                        selectedArtist = null
                                    },
                                    closeAlbum = { selectedAlbum = null },
                                    play = { index ->
                                        selectedArtist?.let {
                                            play(PlaybackSelection(it.untaggedTracks.rows, index))
                                        }
                                    },
                                    playAlbum = { index ->
                                        selectedAlbum?.let { play(it.playbackSelection(index)) }
                                    },
                                    lastRequestedOffset = artistsRequestedOffset,
                                    artistRequestedOffset = artistRequestedOffset,
                                    artistAlbumsRequestedOffset = artistAlbumsRequestedOffset,
                                    albumRequestedOffset = albumRequestedOffset,
                                    loadMoreArtists = ::loadMoreArtists,
                                    loadMoreArtistTracks = ::loadMoreArtistTracks,
                                    loadMoreArtistAlbums = ::loadMoreArtistAlbums,
                                    loadMoreAlbumTracks = ::loadMoreAlbumTracks,
                                )
                                BrowseTab.QUEUE -> NowPlayingQueuePage(
                                    playback = playback,
                                    surfaceState = surfaceState,
                                    surfaceLayout = surfaceLayout,
                                )
                            }
                        }
                    }
                }
            }
        }
        if (!surfaceState.dockMode) {
            if (surfaceLayout == SurfaceLayout.WIDE_SHORT) {
                Row(modifier = Modifier.fillMaxSize()) {
                    LibraryNavigationRail(
                        surfaceLayout = surfaceLayout,
                        shownTab = shownTab,
                        selectTab = ::selectDestination,
                    )
                    libraryScaffold(Modifier.weight(1f))
                }
            } else {
                libraryScaffold(Modifier.fillMaxSize())
            }
        }
        if (surfaceState.dockMode) {
            shownTrack?.let { track ->
                CompositionLocalProvider(
                    LocalNowPlayingActionsEnabled provides !shownTrackIsStale,
                ) {
                    DockModeSurface(track, playback, surfaceState)
                }
            } ?: DockModeWaitingSurface()
        } else {
            AnimatedVisibility(
                // The row is part of the condition, not just of the content: a
                // sheet that slides up around nothing — which is what a stop
                // followed straight away by a new track used to do, the answer
                // for the new row still being read — pops its content in
                // afterwards, with no animation of its own.
                visibleState = nowPlayingSheetState,
                modifier = nowPlayingFrameModifier.testTag("now-playing-frame"),
                enter = slideInVertically(initialOffsetY = { height -> height }) + expandVertically(
                    expandFrom = Alignment.Bottom,
                ),
                exit = slideOutVertically(targetOffsetY = { height -> height }) + shrinkVertically(
                    shrinkTowards = Alignment.Bottom,
                ),
            ) {
                shownTrack?.let { track ->
                    CompositionLocalProvider(
                        LocalNowPlayingActionsEnabled provides !shownTrackIsStale,
                    ) {
                        NowPlayingSheet(
                            track = track,
                            playback = nowPlayingPlayback(),
                            surfaceLayout = surfaceLayout,
                            surfaceState = surfaceState,
                            close = { surfaceState.showNowPlaying(false) },
                        )
                    }
                }
            }
        }
        if (
            surfaceState.dockOfferVisible &&
            surfaceLayout == SurfaceLayout.WIDE_SHORT &&
            shownTrack != null &&
            !surfaceState.dockMode &&
            !settingsVisible
        ) {
            Button(
                onClick = surfaceState::enterDockMode,
                modifier = Modifier
                    .align(Alignment.TopEnd)
                    .padding(12.dp),
            ) {
                Text("Dock mode")
            }
        }
        if (settingsVisible) {
            Surface(
                modifier = Modifier.fillMaxSize(),
                color = MaterialTheme.colorScheme.background,
            ) {
                // Never an empty branch: a full-screen surface with no header
                // and no way back is what a rotation used to leave behind while
                // the settings were being read again.
                when (val current = settingsState) {
                    null -> {
                        BackHandler { surfaceState.showSettings(false) }
                        PlaybackSettingsLoading(close = { surfaceState.showSettings(false) })
                    }
                    else -> SettingsNavigation(
                        state = current,
                        titleCount = state.titles.total,
                        albumCount = state.albumCount,
                        artistCount = state.artists.total,
                        folderName = folderLabel(state.folderUri),
                        themeSelection = themeSelection,
                        onlineSourcesEnabled = onlineSourcesEnabled,
                        setOnlineSourcesEnabled = setOnlineSourcesEnabled,
                        artistPhotoProgress = surfaceState.visibleArtistPhotoProgress,
                        dismissArtistPhotoProgress = surfaceState::dismissArtistPhotoProgress,
                        close = { surfaceState.showSettings(false) },
                        chooseFolder = chooseFolder,
                        rescan = rescan,
                        setEqualizerEnabled = { enabled ->
                            updateSettings { setEqualizerEnabled(enabled) }
                        },
                        replaceEqualizerCurve = { points ->
                            updateSettings { replaceEqualizerCurve(points) }
                        },
                        setGaplessEnabled = { enabled ->
                            updateSettings { setGaplessEnabled(enabled) }
                        },
                        selectTheme = selectTheme,
                    )
                }
            }
        }
    }
}
