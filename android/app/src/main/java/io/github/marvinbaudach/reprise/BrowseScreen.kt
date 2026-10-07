package io.github.marvinbaudach.reprise

import androidx.activity.compose.BackHandler
import androidx.compose.animation.core.MutableTransitionState
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.asPaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.pager.HorizontalPager
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.NavigationBarDefaults
import androidx.compose.material3.Scaffold
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.lifecycle.viewmodel.compose.viewModel
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import uniffi.reprise_android_ffi.AndroidReplayGainMode

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
    searchTitles: suspend (String, LibraryWindowRange) -> LibraryWindow<LibraryTrack>,
    listArtists: suspend (LibraryWindowRange) -> LibraryWindow<LibraryArtist>,
    searchArtists: suspend (String, LibraryWindowRange) -> LibraryWindow<LibraryArtist> =
        { _, range -> listArtists(range) },
    openAlbum: suspend (LibraryAlbum) -> AlbumTrackList,
    listAlbumTracks:
        suspend (LibraryAlbum, LibraryWindowRange) -> LibraryWindow<LibraryTrack>,
    openArtist: suspend (LibraryArtist) -> ArtistTrackList = { artist ->
        ArtistTrackList(artist = artist)
    },
    listArtistTracks:
        suspend (LibraryArtist, LibraryWindowRange) -> LibraryWindow<LibraryTrack> =
        { _, _ -> LibraryWindow.empty() },
    listArtistAlbums:
        suspend (LibraryArtist, LibraryWindowRange) -> LibraryWindow<LibraryAlbum> =
        { _, _ -> LibraryWindow.empty() },
    listArtistUntaggedTracks:
        suspend (LibraryArtist, LibraryWindowRange) -> LibraryWindow<LibraryTrack> =
        { _, _ -> LibraryWindow.empty() },
    loadTrack: (Long, (LibraryTrack?) -> Unit) -> Unit,
    playTracks: (PlaybackSelection, (String) -> Unit) -> Unit,
    loadPlaybackSettings: () -> PlaybackSettingsUiState,
    setEqualizerEnabled: (Boolean) -> PlaybackSettingsUiState,
    replaceEqualizerCurve: (List<EqualizerCurvePoint>) -> PlaybackSettingsUiState,
    setGaplessEnabled: (Boolean) -> PlaybackSettingsUiState,
    themeSelection: MobileThemeSelection,
    selectTheme: (MobileTheme) -> Unit,
    setVolumeKeySkipGestureEnabled: (Boolean) -> PlaybackSettingsUiState = { loadPlaybackSettings() },
    setReplayGainMode: (AndroidReplayGainMode) -> PlaybackSettingsUiState = {
        loadPlaybackSettings()
    },
) {
    val compositionScope = rememberCoroutineScope()
    val libraryQueryScope = remember(state) {
        CoroutineScope(
            compositionScope.coroutineContext +
                SupervisorJob(compositionScope.coroutineContext[Job]),
        )
    }
    DisposableEffect(libraryQueryScope) {
        onDispose { libraryQueryScope.cancel() }
    }
    val readJobs = remember(state) { BrowseReadJobs() }
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
    val visibleTitlesState = remember(state) { mutableStateOf(restored?.titles ?: state.titles) }
    var visibleTitles by visibleTitlesState
    val visibleArtistsState = remember(state) { mutableStateOf(restored?.artists ?: state.artists) }
    var visibleArtists by visibleArtistsState
    var loadedTabs by remember(state) {
        mutableStateOf(restored?.loadedTabs ?: state.loadedTabs)
    }
    val selectedAlbumState = remember(state) { mutableStateOf(restored?.openAlbum) }
    var selectedAlbum by selectedAlbumState
    val selectedArtistState = remember(state) { mutableStateOf(restored?.openArtist) }
    var selectedArtist by selectedArtistState
    val pendingAlbumState = remember(state) { mutableStateOf<LibraryAlbum?>(null) }
    var pendingAlbum by pendingAlbumState
    val pendingArtistState = remember(state) { mutableStateOf<LibraryArtist?>(null) }
    var pendingArtist by pendingArtistState
    val browseErrorState = remember(state) { mutableStateOf(state.message) }
    var browseError by browseErrorState
    val browseErrorOriginState = remember(state) { mutableStateOf<BrowseErrorOrigin?>(null) }
    var browseErrorOrigin by browseErrorOriginState
    var visibleLoadRetryRevision by remember(state, searchText, selectedTab) {
        mutableIntStateOf(0)
    }
    val titlesRequestedOffsetState = remember(state, searchText) { mutableStateOf<Long?>(null) }
    var titlesRequestedOffset by titlesRequestedOffsetState
    val artistsRequestedOffsetState = remember(state, searchText) { mutableStateOf<Long?>(null) }
    var artistsRequestedOffset by artistsRequestedOffsetState
    val albumRequestedOffsetState = remember(state, selectedAlbum?.album) {
        mutableStateOf<Long?>(null)
    }
    var albumRequestedOffset by albumRequestedOffsetState
    val artistRequestedOffsetState = remember(state, selectedArtist?.artist) {
        mutableStateOf<Long?>(null)
    }
    var artistRequestedOffset by artistRequestedOffsetState
    val artistAlbumsRequestedOffsetState = remember(state, selectedArtist?.artist) {
        mutableStateOf<Long?>(null)
    }
    var artistAlbumsRequestedOffset by artistAlbumsRequestedOffsetState
    // Writing `xRequestedOffset` had to move inside `onSuccess` so the
    // sentinel that drives pagination keeps rendering while a read is in
    // flight (see `loadMore*` in [BrowsePaging]), but that leaves the window between
    // "request accepted" and "offset advanced" unguarded: the sentinel can
    // scroll out of view and back in, relaunching its effect with the same
    // key and firing a second read for the same window. This set closes
    // that gap. It is deliberately not Compose state — writing it must not
    // recompose anything, or it would make the sentinel disappear again and
    // cancel the read it is meant to protect.
    val loadsInFlight = remember(state) { mutableSetOf<String>() }
    val surface = BrowseSurfaceGuard(
        surfaceState = surfaceState,
        pendingAlbumState = pendingAlbumState,
        pendingArtistState = pendingArtistState,
        selectedAlbumState = selectedAlbumState,
        selectedArtistState = selectedArtistState,
        browseErrorState = browseErrorState,
        browseErrorOriginState = browseErrorOriginState,
    )
    val nowPlayingExpanded = surfaceState.nowPlayingExpanded
    val settingsVisible = surfaceState.settingsVisible
    val settings = remember { PlaybackSettingsHost() }
    val pagerState = rememberPagerState(
        initialPage = selectedTab.ordinal,
        pageCount = { BrowseTab.entries.size },
    )
    val statusTopInset = remember { mutableStateOf(0.dp) }
    val bottomFrameInset = remember { mutableStateOf(0.dp) }
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
            readJobs.latestAlbumOpen++
            readJobs.latestArtistOpen++
            pendingAlbum = null
            pendingArtist = null
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

    fun openSettings() {
        settings.replace("load", loadPlaybackSettings)
        surfaceState.showSettings(true)
    }

    fun updateSettings(action: () -> PlaybackSettingsUiState) = settings.replace("save", action)

    // Also the reload after a rotation: this runs on entering the composition,
    // and `settingsVisible` is saveable while the settings themselves are not.
    LaunchedEffect(playbackSettingsRevision) {
        if (settingsVisible) {
            settings.replace("refresh", loadPlaybackSettings)
        }
    }

    fun play(requested: PlaybackSelection) {
        // Tracks waiting to be deleted are not played, and not queued.
        val selection = surfaceState.pendingDeletions.visibleSelection(requested) ?: return
        browseError = null
        browseErrorOrigin = null
        playTracks(selection) { message ->
            browseError = message
            browseErrorOrigin = null
        }
    }

    fun openAlbumDetail(album: LibraryAlbum) {
        val request = ++readJobs.latestAlbumOpen
        val parentArtist = selectedArtist?.artist
        pendingAlbum = album
        libraryQueryScope.launch {
            runCatching { openAlbum(album) }
                .onSuccess { detail ->
                    if (
                        request != readJobs.latestAlbumOpen ||
                        !surface.albumOpenIsCurrent(album, parentArtist)
                    ) return@onSuccess
                    pendingAlbum = null
                    selectedAlbum = detail
                    albumRequestedOffset = null
                    surface.clearBrowseError(BrowseErrorOrigin.Artist(parentArtist))
                }
                .onFailure { error ->
                    if (error is CancellationException) throw error
                    if (
                        request == readJobs.latestAlbumOpen &&
                        surface.albumOpenIsCurrent(album, parentArtist)
                    ) {
                        pendingAlbum = null
                        surface.setBrowseError(
                            error.browseDetail("open the album"),
                            BrowseErrorOrigin.Artist(parentArtist),
                        )
                    }
                }
        }
    }

    suspend fun artistsFor(text: String, request: LibraryWindowRange) = if (text.isBlank()) {
        listArtists(request)
    } else {
        searchArtists(text, request)
    }

    suspend fun search(text: String, request: Long) {
        val tab = selectedTab
        // A refinement is a question about a list, so it has to be answered on
        // the list. An open artist page — or the album page nested inside it —
        // would otherwise stay up and answer with that artist's *albums* where
        // the listener asked for artists. The field is shared across the tabs,
        // so this holds wherever the text was typed: the swipe back to Artists
        // lands on whatever is still open there. An empty query is exempt; that
        // is the field being closed again, not a question being asked.
        if (text.isNotBlank()) {
            readJobs.latestAlbumOpen++
            readJobs.latestArtistOpen++
            pendingAlbum = null
            pendingArtist = null
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
        val result = runCatching {
            when (tab) {
                BrowseTab.TITLES -> LoadedTab(
                    titles = searchTitles(text, firstLibraryWindow()),
                )
                BrowseTab.ARTISTS -> LoadedTab(
                    artists = artistsFor(text, firstLibraryWindow()),
                )
                // The queue is an order, not a view of the library, so there is
                // nothing here for a filter to narrow. Selecting it closes the
                // field — see MobileSurfaceViewModel.selectTab.
                BrowseTab.QUEUE -> LoadedTab()
            }
        }
        if (
            request != readJobs.latestSearch ||
            text != surfaceState.searchText ||
            !surface.tabQueryResultIsCurrent(tab)
        ) return
        result.onSuccess { loaded ->
            loaded.titles?.let {
                visibleTitles = it
                titlesRequestedOffset = null
            }
            loaded.artists?.let {
                visibleArtists = it
                artistsRequestedOffset = null
            }
            loadedTabs = setOf(tab)
            surface.clearBrowseError(BrowseErrorOrigin.Tab(tab))
        }
            .onFailure { error ->
                if (error is CancellationException) throw error
                browseError = error.browseDetail("search")
            }
    }

    fun requestSearch(text: String) {
        val request = ++readJobs.latestSearch
        libraryQueryScope.launch { search(text, request) }
    }

    fun toggleSearch() {
        if (searchVisible) {
            surfaceState.closeSearch()
            if (searchText.isNotEmpty()) {
                requestSearch("")
            }
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
            requestSearch(surfaceState.searchText)
        }
    }

    // The tab on screen first, then whatever is still unfetched behind it.
    //
    // Opening the library fills rows for the tab it opens on and no other:
    // `LibrarySession.browseState` hands the rest back through `withoutRows()`,
    // carrying a total but no rows. A swipe draws the next page as soon as it
    // begins and only settles afterwards, so a tab whose rows are still absent
    // is drawn *empty* for the length of the gesture and fills once it lands.
    // Fetching the tab next door while the pager stands still closes that gap
    // before anyone swipes into it.
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
        val query = searchText
        if (!visible) {
            // A prefetch exists to be invisible, so it waits for a moment when
            // nothing is on the line: not the opening frames, where it would
            // compete with the first composition, and not a gesture, where a
            // query landing mid-swipe trades an empty list for a dropped frame.
            delay(NEIGHBOUR_PREFETCH_IDLE_MS)
            snapshotFlow { pagerState.isScrollInProgress }.first { !it }
        }
        val result = runCatching {
            // The rows come off a blocking JNI + SQLite call through the query
            // seam; only this handover to Compose belongs on the main thread.
            when (pendingTab) {
                BrowseTab.TITLES -> LoadedTab(titles = searchTitles(query, firstLibraryWindow()))
                BrowseTab.ARTISTS -> LoadedTab(
                    artists = artistsFor(query, firstLibraryWindow()),
                )
                BrowseTab.QUEUE -> LoadedTab()
            }
        }
        if (query != surfaceState.searchText) return@LaunchedEffect
        result.onSuccess { loaded ->
            loaded.titles?.let { visibleTitles = it }
            loaded.artists?.let { visibleArtists = it }
            loadedTabs = loadedTabs + pendingTab
            if (surface.tabSurfaceIsCurrent(pendingTab)) {
                surface.clearBrowseError(BrowseErrorOrigin.Tab(pendingTab))
            }
        }.onFailure { error ->
            if (error is CancellationException) throw error
            if (visible) {
                surface.setBrowseError(
                    error.browseDetail("load ${pendingTab.label.lowercase()}"),
                    BrowseErrorOrigin.Tab(pendingTab),
                )
            }
            if (visible && visibleLoadRetryRevision == 0) visibleLoadRetryRevision = 1
        }
    }

    val paging = BrowsePaging(
        guard = surface,
        readJobs = readJobs,
        loadsInFlight = loadsInFlight,
        searchText = searchText,
        visibleTitlesState = visibleTitlesState,
        visibleArtistsState = visibleArtistsState,
        selectedAlbumState = selectedAlbumState,
        selectedArtistState = selectedArtistState,
        titlesRequestedOffsetState = titlesRequestedOffsetState,
        artistsRequestedOffsetState = artistsRequestedOffsetState,
        albumRequestedOffsetState = albumRequestedOffsetState,
        artistRequestedOffsetState = artistRequestedOffsetState,
        artistAlbumsRequestedOffsetState = artistAlbumsRequestedOffsetState,
        searchTitles = searchTitles,
        artistsFor = ::artistsFor,
        listAlbumTracks = listAlbumTracks,
        listArtistUntaggedTracks = listArtistUntaggedTracks,
        listArtistAlbums = listArtistAlbums,
    )

    BackHandler(
        enabled = !nowPlayingExpanded && !settingsVisible &&
            (pendingAlbum != null || selectedAlbum != null || pendingArtist != null ||
                selectedArtist != null),
    ) {
        when {
            pendingAlbum != null -> {
                readJobs.latestAlbumOpen++
                pendingAlbum = null
                selectedAlbum = null
            }
            selectedAlbum != null -> {
                readJobs.latestAlbumOpen++
                selectedAlbum = null
            }
            pendingArtist != null -> {
                readJobs.latestAlbumOpen++
                readJobs.latestArtistOpen++
                pendingArtist = null
                selectedArtist = null
            }
            selectedArtist != null -> {
                readJobs.latestAlbumOpen++
                readJobs.latestArtistOpen++
                selectedArtist = null
            }
        }
    }

    val shown = rememberShownTrack(playback, surfaceState, loadTrack)
    val playingTrackId = shown.playingTrackId
    val shownTrack = shown.track
    val shownTrackIsStale = shown.isStale
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
        browseSummary(
            shownTab = shownTab,
            loadedTabs = loadedTabs,
            selectedTab = selectedTab,
            visibleTitles = visibleTitles,
            selectedAlbum = selectedAlbum,
            selectedArtist = selectedArtist,
            visibleArtists = visibleArtists,
        )
    }
    // Lists that resolve their own ids play them through this, and so skip what
    // a pending delete is hiding. Not provided to the snackbar host below: it
    // binds the real transport, and must unbind that same one.
    val realControls = LocalPlaybackControls.current
    val visibleControls = remember(realControls, surfaceState.pendingDeletions) {
        VisibleTracksPlaybackControls(realControls, surfaceState.pendingDeletions)
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
                SideEffect { bottomFrameInset.value = contentPadding.calculateBottomPadding() }
                Column(
                    modifier = Modifier
                        .fillMaxSize()
                        .padding(contentPadding),
                ) {
                    if (searchVisible) {
                        LibrarySearchField(
                            tab = selectedTab,
                            searchText = searchText,
                            search = ::requestSearch,
                            close = ::toggleSearch,
                        )
                    }
                    LibraryArtworkSummaryActions(
                        tab = selectedTab,
                        summary = summary,
                        searching = searchVisible,
                        toggleSearch = ::toggleSearch,
                        rescan = rescan,
                        openSettings = ::openSettings,
                        surfaceState = surfaceState,
                    )
                    CompositionLocalProvider(LocalLibraryStatusTopInset provides statusTopInset) {
                    Box(modifier = Modifier.weight(1f)) {
                        HorizontalPager(
                            state = pagerState,
                            modifier = Modifier
                                .fillMaxSize()
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
                                        loadMore = paging::loadMoreTitles,
                                    )
                                    BrowseTab.ARTISTS -> ArtistsTab(
                                        surfaceLayout = surfaceLayout,
                                        surfaceState = surfaceState,
                                        artists = visibleArtists,
                                        searchText = searchText,
                                        selectedArtist = selectedArtist,
                                        selectedAlbum = selectedAlbum,
                                        pendingAlbum = pendingAlbum,
                                        pendingArtist = pendingArtist,
                                        playback = playback,
                                        openArtist = { artist ->
                                            val request = ++readJobs.latestArtistOpen
                                            pendingArtist = artist
                                            libraryQueryScope.launch {
                                                runCatching { openArtist(artist) }
                                                    .onSuccess { detail ->
                                                        if (
                                                            request != readJobs.latestArtistOpen ||
                                                            !surface.artistOpenIsCurrent(artist)
                                                        ) return@onSuccess
                                                        pendingArtist = null
                                                        selectedArtist = detail
                                                        artistRequestedOffset = null
                                                        artistAlbumsRequestedOffset = null
                                                        surface.clearBrowseError(
                                                            BrowseErrorOrigin.Tab(BrowseTab.ARTISTS),
                                                        )
                                                        surfaceState.closeSearch()
                                                        if (searchText.isNotEmpty()) {
                                                            surfaceState.updateSearch("")
                                                            loadedTabs = emptySet()
                                                        }
                                                    }
                                                    .onFailure { error ->
                                                        if (error is CancellationException) throw error
                                                        if (
                                                            request == readJobs.latestArtistOpen &&
                                                            surface.artistOpenIsCurrent(artist)
                                                        ) {
                                                            pendingArtist = null
                                                            surface.setBrowseError(
                                                                error.browseDetail("open the artist"),
                                                                BrowseErrorOrigin.Tab(BrowseTab.ARTISTS),
                                                            )
                                                        }
                                                    }
                                            }
                                        },
                                        openAlbum = ::openAlbumDetail,
                                        closeArtist = {
                                            readJobs.latestAlbumOpen++
                                            readJobs.latestArtistOpen++
                                            pendingAlbum = null
                                            pendingArtist = null
                                            selectedAlbum = null
                                            selectedArtist = null
                                        },
                                        closeAlbum = {
                                            readJobs.latestAlbumOpen++
                                            pendingAlbum = null
                                            selectedAlbum = null
                                        },
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
                                        loadMoreArtists = paging::loadMoreArtists,
                                        loadMoreArtistTracks = paging::loadMoreArtistTracks,
                                        loadMoreArtistAlbums = paging::loadMoreArtistAlbums,
                                        loadMoreAlbumTracks = paging::loadMoreAlbumTracks,
                                    )
                                    BrowseTab.QUEUE -> NowPlayingQueuePage(
                                        playback = playback,
                                        surfaceState = surfaceState,
                                        surfaceLayout = surfaceLayout,
                                    )
                                }
                            }
                        }
                        LibraryStatusChrome(
                            browseError = browseError,
                            browseErrorOrigin = browseErrorOrigin,
                            surface = surface,
                            dismissBrowseError = {
                                browseError = null
                                browseErrorOrigin = null
                            },
                            surfaceState = surfaceState,
                            playback = playback,
                            nowPlayingSheetState = nowPlayingSheetState,
                            statusTopInset = statusTopInset,
                            detailPageIsTarget = {
                                libraryStatusDetailInsetApplies(
                                    targetPage = BrowseTab.entries[pagerState.targetPage],
                                    detailIsOpen = selectedArtist != null || selectedAlbum != null ||
                                        pendingArtist != null || pendingAlbum != null,
                                )
                            },
                        )
                    }
                    }
                }
            }
        }
        CompositionLocalProvider(LocalPlaybackControls provides visibleControls) {
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
        }
        BrowseNowPlayingLayer(
            surfaceState = surfaceState,
            surfaceLayout = surfaceLayout,
            playback = playback,
            nowPlayingPlayback = nowPlayingPlayback,
            shownTrack = shownTrack,
            shownTrackIsStale = shownTrackIsStale,
            nowPlayingSheetState = nowPlayingSheetState,
            settingsVisible = settingsVisible,
        )
        val sheetBottomInset = NavigationBarDefaults.windowInsets.asPaddingValues().calculateBottomPadding()
        UndoSnackbarHost(surfaceState.pendingDeletions) {
            undoSnackbarClearance(
                nowPlayingOpen = nowPlayingSheetState.currentState || nowPlayingSheetState.targetState,
                layout = surfaceLayout,
                libraryFrameInset = bottomFrameInset.value,
                sheetBottomInset = sheetBottomInset,
            )
        }
        BrowseSettingsOverlay(
            visible = settingsVisible,
            settings = settings,
            state = state,
            surfaceState = surfaceState,
            themeSelection = themeSelection,
            selectTheme = selectTheme,
            chooseFolder = chooseFolder,
            rescan = rescan,
            updateSettings = ::updateSettings,
            setEqualizerEnabled = setEqualizerEnabled,
            replaceEqualizerCurve = replaceEqualizerCurve,
            setGaplessEnabled = setGaplessEnabled,
            setReplayGainMode = setReplayGainMode,
            setVolumeKeySkipGestureEnabled = setVolumeKeySkipGestureEnabled,
        )
    }
}
