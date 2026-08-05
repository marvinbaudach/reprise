package de.reprise.spike

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.expandVertically
import androidx.compose.animation.shrinkVertically
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.viewmodel.compose.viewModel
import de.reprise.spike.settings.SettingsNavigation
import kotlinx.coroutines.delay

/**
 * One answered request for the playing track's row, carrying the id it was
 * asked for. The id is what makes a late answer harmless: it is only ever
 * shown while it is still the track the session reports as playing.
 */
private data class AnsweredTrack(val id: Long, val track: LibraryTrack?)

internal enum class BrowseTab(val label: String) {
    TITLES("Titles"),
    ALBUMS("Albums"),
    ARTISTS("Artists"),
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
    listArtists: (LibraryWindowRange) -> LibraryWindow<LibraryArtist>,
    openAlbum: (LibraryAlbum) -> AlbumTrackList,
    listAlbumTracks: (LibraryAlbum, LibraryWindowRange) -> LibraryWindow<LibraryTrack>,
    loadTrack: (Long, (LibraryTrack?) -> Unit) -> Unit,
    playTracks: (PlaybackSelection, (String) -> Unit) -> Unit,
    loadPlaybackSettings: () -> PlaybackSettingsUiState,
    setEqualizerEnabled: (Boolean) -> PlaybackSettingsUiState,
    replaceEqualizerCurve: (List<EqualizerCurvePoint>) -> PlaybackSettingsUiState,
    setGaplessEnabled: (Boolean) -> PlaybackSettingsUiState,
    themeSelection: MobileThemeSelection,
    selectTheme: (MobileTheme) -> Unit,
) {
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
    var selectedAlbum by remember(state) { mutableStateOf(restored?.openAlbum) }
    var browseError by remember(state) { mutableStateOf(state.message) }
    var titlesRequestedOffset by remember(state, searchText) { mutableStateOf<Long?>(null) }
    var albumsRequestedOffset by remember(state) { mutableStateOf<Long?>(null) }
    var artistsRequestedOffset by remember(state) { mutableStateOf<Long?>(null) }
    var albumRequestedOffset by remember(state, selectedAlbum?.album) { mutableStateOf<Long?>(null) }
    val nowPlayingExpanded = surfaceState.nowPlayingExpanded
    val settingsVisible = surfaceState.settingsVisible
    var settingsState by remember { mutableStateOf<PlaybackSettingsUiState?>(null) }

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

    fun search(text: String) {
        surfaceState.updateSearch(text)
        runCatching { searchTitles(text, firstLibraryWindow()) }
            .onSuccess { tracks ->
                visibleTitles = tracks
                titlesRequestedOffset = null
                browseError = null
            }
            .onFailure { error -> browseError = error.browseDetail("search") }
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
        openAlbum = selectedAlbum,
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
        runCatching { listAlbums(request) }
            .onSuccess { continuation ->
                visibleAlbums = visibleAlbums.append(continuation)
                browseError = null
            }
            .onFailure { error -> browseError = error.browseDetail("load more albums") }
    }

    fun loadMoreArtists(request: LibraryWindowRange) {
        if (visibleArtists.nextRequest(artistsRequestedOffset) != request) return
        artistsRequestedOffset = request.offset
        runCatching { listArtists(request) }
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

    // The row behind the mini player and the sheet is *read*, and reading it
    // takes the same library lock a folder scan holds for its whole walk — so
    // it is asked for from an effect and answered later, never fetched inside
    // the composition. See [TrackLoader].
    var answeredTrack by remember { mutableStateOf<AnsweredTrack?>(null) }
    val playingTrackId = playback.currentTrackId
    LaunchedEffect(playingTrackId, playback.currentTrackUri) {
        if (playingTrackId != null) {
            loadTrack(playingTrackId) { track ->
                answeredTrack = AnsweredTrack(playingTrackId, track)
            }
        }
    }
    // Nothing is shown until the answer for *this* track arrives. Keeping the
    // previous track on screen would be the shorter blank, but it would also be
    // a row that answers for a track that is no longer playing — and the star in
    // the sheet would rate it. A session that stopped therefore blanks the
    // moment it stops, without waiting for anything.
    val currentTrack = answeredTrack?.takeIf { it.id == playingTrackId }?.track
    Box(modifier = Modifier.fillMaxSize()) {
        val libraryScaffold: @Composable (Modifier) -> Unit = { frameModifier ->
            Scaffold(
            modifier = frameModifier,
            containerColor = MaterialTheme.colorScheme.background,
            topBar = {
                LibraryTopAppBar(
                    surfaceLayout = surfaceLayout,
                    searching = searchVisible,
                    toggleSearch = {
                        if (searchVisible) {
                            surfaceState.closeSearch()
                            if (searchText.isNotEmpty()) search("")
                        } else {
                            surfaceState.openSearch()
                        }
                    },
                    rescan = rescan,
                    openSettings = ::openSettings,
                )
            },
            bottomBar = {
                LibraryBottomFrame(
                    surfaceLayout = surfaceLayout,
                    currentTrack = currentTrack,
                    playback = playback,
                    openNowPlaying = { surfaceState.showNowPlaying(true) },
                )
            },
            ) { contentPadding ->
                Column(
                modifier = Modifier
                    .fillMaxSize()
                    .padding(contentPadding),
                ) {
                BrowseFilterChips(
                    surfaceLayout = surfaceLayout,
                    selected = selectedTab,
                    select = { tab ->
                        surfaceState.selectTab(tab)
                    },
                )
                // Both of these are state rather than acknowledgements, so both
                // stand until something supersedes them — see TransientMessage
                // for the distinction and for the third kind.
                browseError?.let { BrowseErrorLine(it) }
                playback.error?.let { BrowseErrorLine(it) }
                when (selectedTab) {
                    BrowseTab.TITLES -> TitlesTab(
                        surfaceLayout = surfaceLayout,
                        surfaceState = surfaceState,
                        tracks = visibleTitles,
                        searchVisible = searchVisible,
                        searchText = searchText,
                        search = ::search,
                        playback = playback,
                        lastRequestedOffset = titlesRequestedOffset,
                        play = { index -> play(PlaybackSelection(visibleTitles.rows, index)) },
                        loadMore = ::loadMoreTitles,
                    )
                    BrowseTab.ALBUMS -> AlbumsTab(
                        surfaceLayout = surfaceLayout,
                        surfaceState = surfaceState,
                        albums = visibleAlbums,
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
                        play = { index -> selectedAlbum?.let { play(it.playbackSelection(index)) } },
                        albumsRequestedOffset = albumsRequestedOffset,
                        albumRequestedOffset = albumRequestedOffset,
                        loadMoreAlbums = ::loadMoreAlbums,
                        loadMoreAlbumTracks = ::loadMoreAlbumTracks,
                    )
                    BrowseTab.ARTISTS -> ArtistsTab(
                        surfaceLayout = surfaceLayout,
                        surfaceState = surfaceState,
                        artists = visibleArtists,
                        lastRequestedOffset = artistsRequestedOffset,
                        loadMore = ::loadMoreArtists,
                    )
                }
                }
            }
        }
        if (!surfaceState.dockMode) {
            if (surfaceLayout == SurfaceLayout.WIDE_SHORT) {
                Row(modifier = Modifier.fillMaxSize()) {
                    LibraryNavigationRail(surfaceLayout)
                    libraryScaffold(Modifier.weight(1f))
                }
            } else {
                libraryScaffold(Modifier.fillMaxSize())
            }
        }
        if (surfaceState.dockMode) {
            currentTrack?.let { track ->
                DockModeSurface(track, playback, surfaceState)
            } ?: DockModeWaitingSurface()
        } else {
            AnimatedVisibility(
                visible = nowPlayingExpanded && currentTrack != null,
                enter = slideInVertically(initialOffsetY = { height -> height }) + expandVertically(
                    expandFrom = Alignment.Bottom,
                ),
                exit = slideOutVertically(targetOffsetY = { height -> height }) + shrinkVertically(
                    shrinkTowards = Alignment.Bottom,
                ),
            ) {
                currentTrack?.let { track ->
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
        if (
            surfaceState.dockOfferVisible &&
            surfaceLayout == SurfaceLayout.WIDE_SHORT &&
            currentTrack != null &&
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
                        playingTrackId = playingTrackId,
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

@Composable
private fun BrowseFilterChips(
    surfaceLayout: SurfaceLayout,
    selected: BrowseTab,
    select: (BrowseTab) -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .horizontalScroll(rememberScrollState())
            .padding(horizontal = 16.dp, vertical = 8.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        BrowseTab.entries.forEach { tab ->
            FilterChip(
                selected = tab == selected,
                onClick = { select(tab) },
                label = { Text(tab.label) },
                leadingIcon = if (tab == selected) {
                    { MaterialSymbol("check", "", sizeSp = 18) }
                } else {
                    null
                },
                modifier = Modifier.height(
                    libraryFrameMetrics(surfaceLayout).filterChipHeightDp.dp,
                ),
                shape = MaterialTheme.shapes.small,
            )
        }
    }
}
