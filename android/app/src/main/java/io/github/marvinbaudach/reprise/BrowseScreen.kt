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
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
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
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch

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

private class BrowseReadJobs {
    var latestSearch = 0L
    var latestAlbumOpen = 0L
    var latestArtistOpen = 0L
}

private sealed interface BrowseErrorOrigin {
    data class Tab(val tab: BrowseTab) : BrowseErrorOrigin
    data class Artist(val artist: LibraryArtist?) : BrowseErrorOrigin
    data class Album(val album: LibraryAlbum) : BrowseErrorOrigin
}

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
    onlineSourcesEnabled: Boolean = false,
    setOnlineSourcesEnabled: (Boolean) -> Unit = {},
    artistPhotoOfferSettled: Boolean = true,
    downloadArtistPhotos: () -> Unit = {},
    declineArtistPhotos: () -> Unit = {},
) {
    val trackAnalysis = LocalTrackAnalysis.current
    val playbackControls = LocalPlaybackControls.current
    val trackArtwork = LocalTrackArtwork.current
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
    var visibleTitles by remember(state) { mutableStateOf(restored?.titles ?: state.titles) }
    var visibleArtists by remember(state) { mutableStateOf(restored?.artists ?: state.artists) }
    var loadedTabs by remember(state) {
        mutableStateOf(restored?.loadedTabs ?: state.loadedTabs)
    }
    var selectedAlbum by remember(state) { mutableStateOf(restored?.openAlbum) }
    var selectedArtist by remember(state) { mutableStateOf(restored?.openArtist) }
    var pendingAlbum by remember(state) { mutableStateOf<LibraryAlbum?>(null) }
    var pendingArtist by remember(state) { mutableStateOf<LibraryArtist?>(null) }
    var browseError by remember(state) { mutableStateOf(state.message) }
    var browseErrorOrigin by remember(state) { mutableStateOf<BrowseErrorOrigin?>(null) }
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
    // Writing `xRequestedOffset` had to move inside `onSuccess` so the
    // sentinel that drives pagination keeps rendering while a read is in
    // flight (see `loadMore*` below), but that leaves the window between
    // "request accepted" and "offset advanced" unguarded: the sentinel can
    // scroll out of view and back in, relaunching its effect with the same
    // key and firing a second read for the same window. This set closes
    // that gap. It is deliberately not Compose state — writing it must not
    // recompose anything, or it would make the sentinel disappear again and
    // cancel the read it is meant to protect.
    val loadsInFlight = remember(state) { mutableSetOf<String>() }
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
        browseErrorOrigin = null
        playTracks(selection) { message ->
            browseError = message
            browseErrorOrigin = null
        }
    }

    fun tabSurfaceIsCurrent(tab: BrowseTab): Boolean =
        surfaceState.selectedTab == tab && pendingAlbum == null && pendingArtist == null &&
            selectedAlbum == null && selectedArtist == null

    fun artistSurfaceIsCurrent(artist: LibraryArtist?): Boolean =
        surfaceState.selectedTab == BrowseTab.ARTISTS &&
            pendingAlbum == null && pendingArtist == null &&
            selectedArtist?.artist == artist && selectedAlbum == null

    fun albumOpenIsCurrent(album: LibraryAlbum, parentArtist: LibraryArtist?): Boolean =
        surfaceState.selectedTab == BrowseTab.ARTISTS && pendingAlbum == album &&
            pendingArtist == null && selectedArtist?.artist == parentArtist

    fun artistOpenIsCurrent(artist: LibraryArtist): Boolean =
        surfaceState.selectedTab == BrowseTab.ARTISTS && pendingArtist == artist &&
            pendingAlbum == null && selectedAlbum == null

    fun albumSurfaceIsCurrent(album: LibraryAlbum): Boolean =
        surfaceState.selectedTab == BrowseTab.ARTISTS && selectedAlbum?.album == album

    fun errorOriginIsCurrent(origin: BrowseErrorOrigin): Boolean = when (origin) {
        is BrowseErrorOrigin.Tab -> tabSurfaceIsCurrent(origin.tab)
        is BrowseErrorOrigin.Artist -> artistSurfaceIsCurrent(origin.artist)
        is BrowseErrorOrigin.Album -> albumSurfaceIsCurrent(origin.album)
    }

    fun setBrowseError(message: String, origin: BrowseErrorOrigin) {
        browseError = message
        browseErrorOrigin = origin
    }

    fun clearBrowseError(origin: BrowseErrorOrigin) {
        if (browseErrorOrigin == null || browseErrorOrigin == origin) {
            browseError = null
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
                        !albumOpenIsCurrent(album, parentArtist)
                    ) return@onSuccess
                    pendingAlbum = null
                    selectedAlbum = detail
                    albumRequestedOffset = null
                    clearBrowseError(BrowseErrorOrigin.Artist(parentArtist))
                }
                .onFailure { error ->
                    if (error is CancellationException) throw error
                    if (
                        request == readJobs.latestAlbumOpen &&
                        albumOpenIsCurrent(album, parentArtist)
                    ) {
                        pendingAlbum = null
                        setBrowseError(
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
            !tabSurfaceIsCurrent(tab)
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
            clearBrowseError(BrowseErrorOrigin.Tab(tab))
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
            if (tabSurfaceIsCurrent(pendingTab)) {
                clearBrowseError(BrowseErrorOrigin.Tab(pendingTab))
            }
        }.onFailure { error ->
            if (error is CancellationException) throw error
            if (visible) {
                setBrowseError(
                    error.browseDetail("load ${pendingTab.label.lowercase()}"),
                    BrowseErrorOrigin.Tab(pendingTab),
                )
            }
            if (visible && visibleLoadRetryRevision == 0) visibleLoadRetryRevision = 1
        }
    }

    // Runs `body` for `key` unless a read for that same key is already in
    // flight, and always clears the key afterwards — including when `body`
    // is cancelled, which is exactly why this is try/finally rather than a
    // clear-on-success inside `onSuccess`.
    suspend fun guardedAgainstDuplicateLoad(key: String, body: suspend () -> Unit) {
        if (!loadsInFlight.add(key)) return
        try {
            body()
        } finally {
            loadsInFlight.remove(key)
        }
    }

    suspend fun loadMoreTitles(request: LibraryWindowRange) {
        if (visibleTitles.nextRequest(titlesRequestedOffset) != request) return
        guardedAgainstDuplicateLoad("titles:${request.offset}") {
            runCatching { searchTitles(searchText, request) }
                .onSuccess { continuation ->
                    titlesRequestedOffset = request.offset
                    visibleTitles = visibleTitles.append(continuation)
                    if (tabSurfaceIsCurrent(BrowseTab.TITLES)) {
                        clearBrowseError(BrowseErrorOrigin.Tab(BrowseTab.TITLES))
                    }
                }
                .onFailure { error ->
                    if (error is CancellationException) throw error
                    if (tabSurfaceIsCurrent(BrowseTab.TITLES)) {
                        setBrowseError(
                            error.browseDetail("load more titles"),
                            BrowseErrorOrigin.Tab(BrowseTab.TITLES),
                        )
                    }
                }
        }
    }

    suspend fun loadMoreArtists(request: LibraryWindowRange) {
        if (visibleArtists.nextRequest(artistsRequestedOffset) != request) return
        guardedAgainstDuplicateLoad("artists:${request.offset}") {
            runCatching { artistsFor(searchText, request) }
                .onSuccess { continuation ->
                    artistsRequestedOffset = request.offset
                    visibleArtists = visibleArtists.append(continuation)
                    if (tabSurfaceIsCurrent(BrowseTab.ARTISTS)) {
                        clearBrowseError(BrowseErrorOrigin.Tab(BrowseTab.ARTISTS))
                    }
                }
                .onFailure { error ->
                    if (error is CancellationException) throw error
                    if (tabSurfaceIsCurrent(BrowseTab.ARTISTS)) {
                        setBrowseError(
                            error.browseDetail("load more artists"),
                            BrowseErrorOrigin.Tab(BrowseTab.ARTISTS),
                        )
                    }
                }
        }
    }

    suspend fun loadMoreAlbumTracks(request: LibraryWindowRange) {
        val detail = selectedAlbum ?: return
        if (detail.tracks.nextRequest(albumRequestedOffset) != request) return
        guardedAgainstDuplicateLoad("album-tracks:${request.offset}") {
            runCatching { listAlbumTracks(detail.album, request) }
                .onSuccess { continuation ->
                    albumRequestedOffset = request.offset
                    selectedAlbum = detail.copy(tracks = detail.tracks.append(continuation))
                    if (albumSurfaceIsCurrent(detail.album)) {
                        clearBrowseError(BrowseErrorOrigin.Album(detail.album))
                    }
                }
                .onFailure { error ->
                    if (error is CancellationException) throw error
                    if (albumSurfaceIsCurrent(detail.album)) {
                        setBrowseError(
                            error.browseDetail("load more album tracks"),
                            BrowseErrorOrigin.Album(detail.album),
                        )
                    }
                }
        }
    }

    suspend fun loadMoreArtistTracks(request: LibraryWindowRange) {
        val detail = selectedArtist ?: return
        if (detail.untaggedTracks.nextRequest(artistRequestedOffset) != request) return
        guardedAgainstDuplicateLoad("artist-tracks:${request.offset}") {
            runCatching { listArtistUntaggedTracks(detail.artist, request) }
                .onSuccess { continuation ->
                    artistRequestedOffset = request.offset
                    selectedArtist = detail.copy(
                        untaggedTracks = detail.untaggedTracks.append(continuation),
                    )
                    if (artistSurfaceIsCurrent(detail.artist)) {
                        clearBrowseError(BrowseErrorOrigin.Artist(detail.artist))
                    }
                }
                .onFailure { error ->
                    if (error is CancellationException) throw error
                    if (artistSurfaceIsCurrent(detail.artist)) {
                        setBrowseError(
                            error.browseDetail("load more other titles"),
                            BrowseErrorOrigin.Artist(detail.artist),
                        )
                    }
                }
        }
    }

    suspend fun loadMoreArtistAlbums(request: LibraryWindowRange) {
        val detail = selectedArtist ?: return
        if (detail.albums.nextRequest(artistAlbumsRequestedOffset) != request) return
        guardedAgainstDuplicateLoad("artist-albums:${request.offset}") {
            runCatching { listArtistAlbums(detail.artist, request) }
                .onSuccess { continuation ->
                    artistAlbumsRequestedOffset = request.offset
                    selectedArtist = detail.copy(albums = detail.albums.append(continuation))
                    if (artistSurfaceIsCurrent(detail.artist)) {
                        clearBrowseError(BrowseErrorOrigin.Artist(detail.artist))
                    }
                }
                .onFailure { error ->
                    if (error is CancellationException) throw error
                    if (artistSurfaceIsCurrent(detail.artist)) {
                        setBrowseError(
                            error.browseDetail("load more artist albums"),
                            BrowseErrorOrigin.Artist(detail.artist),
                        )
                    }
                }
        }
    }

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
                            search = ::requestSearch,
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
                    browseError
                        ?.takeIf {
                            browseErrorOrigin?.let(::errorOriginIsCurrent) != false
                        }
                        ?.let { BrowseErrorLine(it) }
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
                                                        !artistOpenIsCurrent(artist)
                                                    ) return@onSuccess
                                                    pendingArtist = null
                                                    selectedArtist = detail
                                                    artistRequestedOffset = null
                                                    artistAlbumsRequestedOffset = null
                                                    clearBrowseError(
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
                                                        artistOpenIsCurrent(artist)
                                                    ) {
                                                        pendingArtist = null
                                                        setBrowseError(
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
