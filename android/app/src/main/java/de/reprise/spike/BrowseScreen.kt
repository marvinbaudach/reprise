package de.reprise.spike

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.expandVertically
import androidx.compose.animation.shrinkVertically
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
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
import de.reprise.spike.settings.SettingsNavigation
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.drop

/**
 * One answered request for the playing track's row, carrying the id it was
 * asked for. The id says whether the retained row still describes the track
 * the session reports as playing, so actions can be disabled during a change.
 */
private data class AnsweredTrack(val id: Long, val track: LibraryTrack?)

internal enum class BrowseTab(val label: String, val symbol: String) {
    TITLES("Titles", "library_music"),
    ARTISTS("Artists", "artist"),
    ALBUMS("Albums", "album"),
    FAVOURITES("Favourites", "favorite"),
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
    playback: PlaybackUiState,
    playbackSettingsRevision: Long,
    surfaceLayout: SurfaceLayout = SurfaceLayout.STACKED,
    surfaceState: MobileSurfaceViewModel = viewModel(),
    chooseFolder: () -> Unit,
    rescan: () -> Unit,
    searchTitles: (String, LibraryWindowRange) -> LibraryWindow<LibraryTrack>,
    listAlbums: (LibraryWindowRange) -> LibraryWindow<LibraryAlbum>,
    searchAlbums: (String, LibraryWindowRange) -> LibraryWindow<LibraryAlbum> =
        { _, range -> listAlbums(range) },
    listArtists: (LibraryWindowRange) -> LibraryWindow<LibraryArtist>,
    searchArtists: (String, LibraryWindowRange) -> LibraryWindow<LibraryArtist> =
        { _, range -> listArtists(range) },
    openAlbum: (LibraryAlbum) -> AlbumTrackList,
    listAlbumTracks: (LibraryAlbum, LibraryWindowRange) -> LibraryWindow<LibraryTrack>,
    openArtist: (LibraryArtist) -> ArtistTrackList = { artist ->
        ArtistTrackList(artist, LibraryWindow.empty())
    },
    listArtistTracks: (LibraryArtist, LibraryWindowRange) -> LibraryWindow<LibraryTrack> =
        { _, _ -> LibraryWindow.empty() },
    listFavourites: (LibraryWindowRange) -> LibraryWindow<LibraryTrack> =
        { LibraryWindow.empty() },
    searchFavourites: (String, LibraryWindowRange) -> LibraryWindow<LibraryTrack> =
        { _, range -> listFavourites(range) },
    loadTrack: (Long, (LibraryTrack?) -> Unit) -> Unit,
    playTracks: (PlaybackSelection, (String) -> Unit) -> Unit,
    loadPlaybackSettings: () -> PlaybackSettingsUiState,
    setEqualizerEnabled: (Boolean) -> PlaybackSettingsUiState,
    replaceEqualizerCurve: (List<EqualizerCurvePoint>) -> PlaybackSettingsUiState,
    setGaplessEnabled: (Boolean) -> PlaybackSettingsUiState,
    themeSelection: MobileThemeSelection,
    selectTheme: (MobileTheme) -> Unit,
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
    var visibleAlbums by remember(state) { mutableStateOf(restored?.albums ?: state.albums) }
    var visibleArtists by remember(state) { mutableStateOf(restored?.artists ?: state.artists) }
    var visibleFavourites by remember(state) {
        mutableStateOf(restored?.favourites ?: state.favourites)
    }
    var loadedTabs by remember(state) {
        mutableStateOf(restored?.loadedTabs ?: state.loadedTabs)
    }
    var selectedAlbum by remember(state) { mutableStateOf(restored?.openAlbum) }
    var selectedArtist by remember(state) { mutableStateOf(restored?.openArtist) }
    var browseError by remember(state) { mutableStateOf(state.message) }
    var titlesRequestedOffset by remember(state, searchText) { mutableStateOf<Long?>(null) }
    var albumsRequestedOffset by remember(state, searchText) { mutableStateOf<Long?>(null) }
    var artistsRequestedOffset by remember(state, searchText) { mutableStateOf<Long?>(null) }
    var favouritesRequestedOffset by remember(state, searchText) { mutableStateOf<Long?>(null) }
    var albumRequestedOffset by remember(state, selectedAlbum?.album) { mutableStateOf<Long?>(null) }
    var artistRequestedOffset by remember(state, selectedArtist?.artist) {
        mutableStateOf<Long?>(null)
    }
    val nowPlayingExpanded = surfaceState.nowPlayingExpanded
    val settingsVisible = surfaceState.settingsVisible
    var settingsState by remember { mutableStateOf<PlaybackSettingsUiState?>(null) }
    val pagerState = rememberPagerState(
        initialPage = selectedTab.ordinal,
        pageCount = { BrowseTab.entries.size },
    )

    fun selectDestination(tab: BrowseTab) {
        if (tab == BrowseTab.FAVOURITES) {
            loadedTabs -= BrowseTab.FAVOURITES
        }
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

    fun albumsFor(text: String, request: LibraryWindowRange) = if (text.isBlank()) {
        listAlbums(request)
    } else {
        searchAlbums(text, request)
    }

    fun artistsFor(text: String, request: LibraryWindowRange) = if (text.isBlank()) {
        listArtists(request)
    } else {
        searchArtists(text, request)
    }

    fun favouritesFor(text: String, request: LibraryWindowRange) = if (text.isBlank()) {
        listFavourites(request)
    } else {
        searchFavourites(text, request)
    }

    fun search(text: String) {
        // Only the tab this fills can be said to hold the refinement afterwards,
        // so the whole set goes first and exactly one comes back. Asking whether
        // the *text* changed would not be enough: a rescan re-enters here with
        // the same query while handing over four freshly loaded — and unfiltered
        // — windows that claim to be loaded already.
        loadedTabs = emptySet()
        surfaceState.updateSearch(text)
        runCatching {
            when (selectedTab) {
                BrowseTab.TITLES -> searchTitles(text, firstLibraryWindow()).also { window ->
                    visibleTitles = window
                    titlesRequestedOffset = null
                }
                BrowseTab.ALBUMS -> albumsFor(text, firstLibraryWindow()).also { window ->
                    visibleAlbums = window
                    albumsRequestedOffset = null
                }
                BrowseTab.ARTISTS -> artistsFor(text, firstLibraryWindow()).also { window ->
                    visibleArtists = window
                    artistsRequestedOffset = null
                }
                BrowseTab.FAVOURITES -> favouritesFor(text, firstLibraryWindow()).also { window ->
                    visibleFavourites = window
                    favouritesRequestedOffset = null
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
    // back. Handed over from the composition itself rather than from each of
    // the five places that change a window, so the two cannot drift apart.
    //
    // Assembled *here*, in this function's own scope, and not inside the effect
    // below: a state value read only from an inner lambda invalidates only that
    // lambda, and an effect in this scope would then keep handing back the
    // window it saw first — 200 rows, however many the listener had paged in.
    val loaded = LoadedLibraryWindows(
        titles = visibleTitles,
        albums = visibleAlbums,
        artists = visibleArtists,
        favourites = visibleFavourites,
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

    LaunchedEffect(selectedTab, state, loadedTabs, searchText) {
        if (selectedTab == BrowseTab.QUEUE || selectedTab in loadedTabs) return@LaunchedEffect
        runCatching {
            when (selectedTab) {
                BrowseTab.TITLES -> searchTitles(searchText, firstLibraryWindow())
                    .also { visibleTitles = it }
                BrowseTab.ARTISTS -> artistsFor(searchText, firstLibraryWindow())
                    .also { visibleArtists = it }
                BrowseTab.ALBUMS -> albumsFor(searchText, firstLibraryWindow())
                    .also { visibleAlbums = it }
                BrowseTab.FAVOURITES -> favouritesFor(searchText, firstLibraryWindow())
                    .also { visibleFavourites = it }
                BrowseTab.QUEUE -> Unit
            }
        }.onSuccess {
            loadedTabs = loadedTabs + selectedTab
            browseError = null
        }.onFailure { error ->
            browseError = error.browseDetail("load ${selectedTab.label.lowercase()}")
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

    fun loadMoreAlbums(request: LibraryWindowRange) {
        if (visibleAlbums.nextRequest(albumsRequestedOffset) != request) return
        albumsRequestedOffset = request.offset
        runCatching { albumsFor(searchText, request) }
            .onSuccess { continuation ->
                visibleAlbums = visibleAlbums.append(continuation)
                browseError = null
            }
            .onFailure { error -> browseError = error.browseDetail("load more albums") }
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
        if (detail.tracks.nextRequest(artistRequestedOffset) != request) return
        artistRequestedOffset = request.offset
        runCatching { listArtistTracks(detail.artist, request) }
            .onSuccess { continuation ->
                selectedArtist = detail.copy(tracks = detail.tracks.append(continuation))
                browseError = null
            }
            .onFailure { error -> browseError = error.browseDetail("load more artist tracks") }
    }

    fun loadMoreFavourites(request: LibraryWindowRange) {
        if (visibleFavourites.nextRequest(favouritesRequestedOffset) != request) return
        favouritesRequestedOffset = request.offset
        runCatching { favouritesFor(searchText, request) }
            .onSuccess { continuation ->
                visibleFavourites = visibleFavourites.append(continuation)
                browseError = null
            }
            .onFailure { error -> browseError = error.browseDetail("load more favourites") }
    }

    BackHandler(
        enabled = !nowPlayingExpanded && !settingsVisible &&
            (selectedAlbum != null || selectedArtist != null),
    ) {
        when {
            selectedArtist != null -> selectedArtist = null
            selectedAlbum != null -> selectedAlbum = null
        }
    }

    // The row behind the mini player and the sheet is *read*, and reading it
    // takes the same library lock a folder scan holds for its whole walk — so
    // it is asked for from an effect and answered later, never fetched inside
    // the composition. See [TrackLoader].
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
    val summary = when (selectedTab) {
        BrowseTab.TITLES -> visibleTitles.visibleCountLabel("title", "titles")
        BrowseTab.ARTISTS -> selectedArtist?.tracks
            ?.visibleCountLabel("track", "tracks")
            ?: visibleArtists.visibleCountLabel("artist", "artists")
        BrowseTab.ALBUMS -> selectedAlbum?.tracks
            ?.visibleCountLabel("track", "tracks")
            ?: visibleAlbums.visibleCountLabel("album", "albums")
        BrowseTab.FAVOURITES -> visibleFavourites.visibleCountLabel("favourite", "favourites")
        BrowseTab.QUEUE -> "Queue"
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
                        selectedTab = selectedTab,
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
                        summary = summary,
                        searching = searchVisible,
                        toggleSearch = ::toggleSearch,
                        rescan = rescan,
                        openSettings = ::openSettings,
                    )
                    // Both of these are state rather than acknowledgements, so both
                    // stand until something supersedes them — see TransientMessage
                    // for the distinction and for the third kind.
                    browseError?.let { BrowseErrorLine(it) }
                    playback.error?.let { BrowseErrorLine(it) }
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
                                BrowseTab.ALBUMS -> AlbumsTab(
                                    surfaceLayout = surfaceLayout,
                                    surfaceState = surfaceState,
                                    albums = visibleAlbums,
                                    searchText = searchText,
                                    selectedAlbum = selectedAlbum,
                                    playback = playback,
                                    openAlbum = { album ->
                                        runCatching { openAlbum(album) }
                                            .onSuccess { detail ->
                                                selectedAlbum = detail
                                                albumRequestedOffset = null
                                                browseError = null
                                            }
                                            .onFailure { error ->
                                                browseError = error.browseDetail("open the album")
                                            }
                                    },
                                    closeAlbum = { selectedAlbum = null },
                                    play = { index ->
                                        selectedAlbum?.let { play(it.playbackSelection(index)) }
                                    },
                                    albumsRequestedOffset = albumsRequestedOffset,
                                    albumRequestedOffset = albumRequestedOffset,
                                    loadMoreAlbums = ::loadMoreAlbums,
                                    loadMoreAlbumTracks = ::loadMoreAlbumTracks,
                                )
                                BrowseTab.ARTISTS -> ArtistsTab(
                                    surfaceLayout = surfaceLayout,
                                    surfaceState = surfaceState,
                                    artists = visibleArtists,
                                    searchText = searchText,
                                    selectedArtist = selectedArtist,
                                    playback = playback,
                                    openArtist = { artist ->
                                        runCatching { openArtist(artist) }
                                            .onSuccess { detail ->
                                                selectedArtist = detail
                                                artistRequestedOffset = null
                                                browseError = null
                                            }
                                            .onFailure { error ->
                                                browseError = error.browseDetail("open the artist")
                                            }
                                    },
                                    closeArtist = { selectedArtist = null },
                                    play = { index ->
                                        selectedArtist?.let { play(it.playbackSelection(index)) }
                                    },
                                    lastRequestedOffset = artistsRequestedOffset,
                                    artistRequestedOffset = artistRequestedOffset,
                                    loadMoreArtists = ::loadMoreArtists,
                                    loadMoreArtistTracks = ::loadMoreArtistTracks,
                                )
                                BrowseTab.FAVOURITES -> FavouritesTab(
                                    surfaceLayout = surfaceLayout,
                                    surfaceState = surfaceState,
                                    tracks = visibleFavourites,
                                    searchText = searchText,
                                    playback = playback,
                                    lastRequestedOffset = favouritesRequestedOffset,
                                    play = { index ->
                                        play(PlaybackSelection(visibleFavourites.rows, index))
                                    },
                                    loadMore = ::loadMoreFavourites,
                                    removeFavourite = { track ->
                                        val removal = visibleFavourites.removeTrack(
                                            track.id,
                                            favouritesRequestedOffset,
                                        )
                                        visibleFavourites = removal.window
                                        favouritesRequestedOffset = removal.lastRequestedOffset
                                    },
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
                        selectedTab = selectedTab,
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
                DockModeSurface(track, playback, surfaceState)
            } ?: DockModeWaitingSurface()
        } else {
            AnimatedVisibility(
                visible = nowPlayingExpanded && playingTrackId != null,
                modifier = nowPlayingFrameModifier,
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
                            playback = playback,
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
                        albumCount = state.albums.total,
                        artistCount = state.artists.total,
                        folderName = folderLabel(state.folderUri),
                        themeSelection = themeSelection,
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

@Composable
private fun BrowseErrorLine(message: String) {
    Text(
        text = message,
        color = MaterialTheme.colorScheme.error,
        modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp),
    )
}
