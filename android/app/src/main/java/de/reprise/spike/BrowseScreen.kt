package de.reprise.spike

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
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp

private enum class BrowseTab(val label: String) {
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
    chooseFolder: () -> Unit,
    rescan: () -> Unit,
    searchTitles: (String, LibraryWindowRange) -> LibraryWindow<LibraryTrack>,
    listAlbums: (LibraryWindowRange) -> LibraryWindow<LibraryAlbum>,
    listArtists: (LibraryWindowRange) -> LibraryWindow<LibraryArtist>,
    openAlbum: (LibraryAlbum) -> AlbumTrackList,
    listAlbumTracks: (LibraryAlbum, LibraryWindowRange) -> LibraryWindow<LibraryTrack>,
    playTracks: (PlaybackSelection, (String) -> Unit) -> Unit,
    themeSelection: MobileThemeSelection,
    selectTheme: (MobileTheme) -> Unit,
) {
    var selectedTab by remember { mutableStateOf(BrowseTab.TITLES) }
    var searchVisible by remember { mutableStateOf(false) }
    var searchText by remember(state) { mutableStateOf("") }
    var visibleTitles by remember(state) { mutableStateOf(state.titles) }
    var visibleAlbums by remember(state) { mutableStateOf(state.albums) }
    var visibleArtists by remember(state) { mutableStateOf(state.artists) }
    var selectedAlbum by remember(state) { mutableStateOf<AlbumTrackList?>(null) }
    var activeSelection by remember { mutableStateOf<PlaybackSelection?>(null) }
    var browseError by remember(state) { mutableStateOf(state.message) }
    var titlesRequestedOffset by remember(state, searchText) { mutableStateOf<Long?>(null) }
    var albumsRequestedOffset by remember(state) { mutableStateOf<Long?>(null) }
    var artistsRequestedOffset by remember(state) { mutableStateOf<Long?>(null) }
    var albumRequestedOffset by remember(state, selectedAlbum?.album) { mutableStateOf<Long?>(null) }
    // Saveable, not remembered: a rotation recreates the activity, and a
    // sheet the user opened is not something the device orientation gets to
    // close.
    var nowPlayingExpanded by rememberSaveable { mutableStateOf(false) }

    fun play(selection: PlaybackSelection) {
        browseError = null
        activeSelection = selection
        playTracks(selection) { message -> browseError = message }
    }

    fun search(text: String) {
        searchText = text
        runCatching { searchTitles(text, firstLibraryWindow()) }
            .onSuccess { tracks ->
                visibleTitles = tracks
                titlesRequestedOffset = null
                browseError = null
            }
            .onFailure { error -> browseError = error.browseDetail("search") }
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

    val currentTrack = activeSelection?.currentTrack(playback)
    Box(modifier = Modifier.fillMaxSize()) {
        Scaffold(
            containerColor = MaterialTheme.colorScheme.background,
            topBar = {
                LibraryTopAppBar(
                    searching = searchVisible,
                    toggleSearch = {
                        searchVisible = !searchVisible
                        selectedTab = BrowseTab.TITLES
                        if (!searchVisible && searchText.isNotEmpty()) search("")
                    },
                    rescan = rescan,
                    chooseFolder = chooseFolder,
                    themeSelection = themeSelection,
                    selectTheme = selectTheme,
                )
            },
            bottomBar = {
                LibraryBottomFrame(
                    currentTrack = currentTrack,
                    playback = playback,
                    openNowPlaying = { nowPlayingExpanded = true },
                )
            },
        ) { contentPadding ->
            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .padding(contentPadding),
            ) {
                BrowseFilterChips(
                    selected = selectedTab,
                    select = { tab ->
                        selectedTab = tab
                        if (tab != BrowseTab.TITLES) searchVisible = false
                    },
                )
                // Both of these are state rather than acknowledgements, so both
                // stand until something supersedes them — see TransientMessage
                // for the distinction and for the third kind.
                browseError?.let { BrowseErrorLine(it) }
                playback.error?.let { BrowseErrorLine(it) }
                when (selectedTab) {
                    BrowseTab.TITLES -> TitlesTab(
                        tracks = visibleTitles,
                        searchVisible = searchVisible,
                        searchText = searchText,
                        search = ::search,
                        activeSelection = activeSelection,
                        playback = playback,
                        lastRequestedOffset = titlesRequestedOffset,
                        play = { index -> play(PlaybackSelection(visibleTitles.rows, index)) },
                        loadMore = ::loadMoreTitles,
                    )
                    BrowseTab.ALBUMS -> AlbumsTab(
                        albums = visibleAlbums,
                        selectedAlbum = selectedAlbum,
                        activeSelection = activeSelection,
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
                        artists = visibleArtists,
                        lastRequestedOffset = artistsRequestedOffset,
                        loadMore = ::loadMoreArtists,
                    )
                }
            }
        }
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
                    close = { nowPlayingExpanded = false },
                )
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
private fun BrowseFilterChips(selected: BrowseTab, select: (BrowseTab) -> Unit) {
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
                modifier = Modifier.height(libraryFrameMetrics.filterChipHeightDp.dp),
                shape = MaterialTheme.shapes.small,
            )
        }
    }
}
