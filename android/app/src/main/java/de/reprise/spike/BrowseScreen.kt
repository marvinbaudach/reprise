package de.reprise.spike

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.expandVertically
import androidx.compose.animation.shrinkVertically
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.FilterChip
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import uniffi.reprise_android_ffi.AndroidRepeatMode

private enum class BrowseTab(val label: String) {
    TITLES("Titles"),
    ALBUMS("Albums"),
    ARTISTS("Artists"),
}

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
    togglePause: () -> Unit,
    next: () -> Unit,
    previous: () -> Unit,
    seekTo: (Long) -> Unit,
    setShuffle: (Boolean) -> Unit,
    setRepeat: (AndroidRepeatMode) -> Unit,
    setRating: (Long, Int) -> String?,
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
            )
        },
        bottomBar = {
            LibraryBottomFrame(
                currentTrack = currentTrack,
                playback = playback,
                togglePause = togglePause,
                next = next,
                previous = previous,
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
            browseError?.let {
                Text(
                    text = it,
                    color = MaterialTheme.colorScheme.error,
                    modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp),
                )
            }
            playback.error?.let {
                Text(
                    text = it,
                    color = MaterialTheme.colorScheme.error,
                    modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp),
                )
            }
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
                    seekTo = seekTo,
                    togglePause = togglePause,
                    next = next,
                    previous = previous,
                    setShuffle = setShuffle,
                    setRepeat = setRepeat,
                    setRating = setRating,
                )
            }
        }
    }
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

@Composable
private fun TitlesTab(
    tracks: LibraryWindow<LibraryTrack>,
    searchVisible: Boolean,
    searchText: String,
    search: (String) -> Unit,
    activeSelection: PlaybackSelection?,
    playback: PlaybackUiState,
    lastRequestedOffset: Long?,
    play: (Int) -> Unit,
    loadMore: (LibraryWindowRange) -> Unit,
) {
    Column(modifier = Modifier.fillMaxSize()) {
        if (searchVisible) {
            OutlinedTextField(
                value = searchText,
                onValueChange = search,
                label = { Text("Search titles") },
                leadingIcon = { MaterialSymbol("search", "") },
                singleLine = true,
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 16.dp, vertical = 4.dp),
            )
        }
        Text(
            text = tracks.visibleCountLabel("title", "titles"),
            style = MaterialTheme.typography.labelMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp),
        )
        if (tracks.rows.isEmpty()) {
            Text(
                text = if (searchText.isEmpty()) "No tracks found in this folder." else "No matches.",
                modifier = Modifier.padding(16.dp),
            )
        } else {
            TrackRows(
                tracks = tracks,
                activeSelection = activeSelection,
                playback = playback,
                lastRequestedOffset = lastRequestedOffset,
                play = play,
                loadMore = loadMore,
            )
        }
    }
}

@Composable
private fun AlbumsTab(
    albums: LibraryWindow<LibraryAlbum>,
    selectedAlbum: AlbumTrackList?,
    activeSelection: PlaybackSelection?,
    playback: PlaybackUiState,
    openAlbum: (LibraryAlbum) -> Unit,
    closeAlbum: () -> Unit,
    play: (Int) -> Unit,
    albumsRequestedOffset: Long?,
    albumRequestedOffset: Long?,
    loadMoreAlbums: (LibraryWindowRange) -> Unit,
    loadMoreAlbumTracks: (LibraryWindowRange) -> Unit,
) {
    if (selectedAlbum != null) {
        Column(modifier = Modifier.fillMaxSize()) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .clickable(onClick = closeAlbum)
                    .padding(horizontal = 16.dp, vertical = 8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                MaterialSymbol("arrow_back", "Back to albums")
                Text(selectedAlbum.album.title, style = MaterialTheme.typography.titleLarge)
            }
            Text(
                selectedAlbum.album.artist,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.padding(horizontal = 16.dp),
            )
            Text(
                selectedAlbum.tracks.visibleCountLabel("track", "tracks"),
                style = MaterialTheme.typography.labelMedium,
                modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp),
            )
            if (selectedAlbum.tracks.rows.isEmpty()) {
                Text("No tracks in this album.", modifier = Modifier.padding(16.dp))
            } else {
                TrackRows(
                    tracks = selectedAlbum.tracks,
                    activeSelection = activeSelection,
                    playback = playback,
                    lastRequestedOffset = albumRequestedOffset,
                    play = play,
                    loadMore = loadMoreAlbumTracks,
                )
            }
        }
        return
    }

    if (albums.rows.isEmpty()) {
        Text("No albums found in this folder.", modifier = Modifier.padding(16.dp))
        return
    }
    Column(modifier = Modifier.fillMaxSize()) {
        Text(
            albums.visibleCountLabel("album", "albums"),
            style = MaterialTheme.typography.labelMedium,
            modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp),
        )
        LazyColumn(modifier = Modifier.fillMaxSize()) {
            items(albums.rows, key = { album -> "${album.artist}\u0000${album.title}" }) { album ->
                ListItem(
                    headlineContent = { Text(album.title) },
                    supportingContent = { Text(album.details()) },
                    trailingContent = { Text(formatDuration(album.totalDurationMs)) },
                    modifier = Modifier
                        .fillMaxWidth()
                        .clickable { openAlbum(album) },
                )
                HorizontalDivider()
            }
            windowContinuation(albums, albumsRequestedOffset, loadMoreAlbums)
        }
    }
}

@Composable
private fun ArtistsTab(
    artists: LibraryWindow<LibraryArtist>,
    lastRequestedOffset: Long?,
    loadMore: (LibraryWindowRange) -> Unit,
) {
    if (artists.rows.isEmpty()) {
        Text("No artists found in this folder.", modifier = Modifier.padding(16.dp))
        return
    }
    Column(modifier = Modifier.fillMaxSize()) {
        Text(
            artists.visibleCountLabel("artist", "artists"),
            style = MaterialTheme.typography.labelMedium,
            modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp),
        )
        LazyColumn(modifier = Modifier.fillMaxSize()) {
            items(artists.rows, key = LibraryArtist::name) { artist ->
                ListItem(
                    headlineContent = { Text(artist.name) },
                    supportingContent = { Text(artist.details()) },
                )
                HorizontalDivider()
            }
            windowContinuation(artists, lastRequestedOffset, loadMore)
        }
    }
}

@Composable
private fun TrackRows(
    tracks: LibraryWindow<LibraryTrack>,
    activeSelection: PlaybackSelection?,
    playback: PlaybackUiState,
    lastRequestedOffset: Long?,
    play: (Int) -> Unit,
    loadMore: (LibraryWindowRange) -> Unit,
) {
    LazyColumn(modifier = Modifier.fillMaxSize()) {
        items(
            items = trackListContent(tracks, lastRequestedOffset),
            key = { content ->
                when (content) {
                    is TrackListContent.Row -> "track-${content.track.uri}"
                    is TrackListContent.Continuation -> "load-window-${content.request.offset}"
                }
            },
        ) { content ->
            when (content) {
                is TrackListContent.Row -> LibraryTrackRow(
                    track = content.track,
                    presentation = content.track.playbackPresentation(activeSelection, playback),
                    play = { play(content.index) },
                )
                is TrackListContent.Continuation -> {
                    LaunchedEffect(content.request.offset) { loadMore(content.request) }
                    Text(
                        text = "Loading…",
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        modifier = Modifier.padding(16.dp),
                    )
                }
            }
        }
    }
}

@Composable
private fun LibraryTrackRow(
    track: LibraryTrack,
    presentation: TrackPlaybackPresentation,
    play: () -> Unit,
) {
    Surface(
        modifier = Modifier
            .fillMaxWidth()
            .height(libraryFrameMetrics.trackRowHeightDp.dp)
            .clickable(onClick = play),
        color = if (presentation.isCurrent) {
            MaterialTheme.colorScheme.primary.copy(alpha = 0.08f)
        } else {
            MaterialTheme.colorScheme.background
        },
    ) {
        Box {
            Row(
                modifier = Modifier
                    .fillMaxSize()
                    .padding(horizontal = 16.dp, vertical = 8.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                TrackCover(
                    trackUri = track.uri,
                    size = libraryFrameMetrics.trackCoverSizeDp,
                )
                Column(modifier = Modifier.weight(1f)) {
                    Text(
                        text = track.title,
                        style = MaterialTheme.typography.titleMedium,
                        color = if (presentation.isCurrent) {
                            MaterialTheme.colorScheme.onPrimaryContainer
                        } else {
                            MaterialTheme.colorScheme.onBackground
                        },
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Text(
                            text = track.details(),
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                            modifier = Modifier.weight(1f),
                        )
                        TrackRating(track.rating)
                    }
                }
                Column(
                    modifier = Modifier.width(48.dp),
                    horizontalAlignment = Alignment.End,
                    verticalArrangement = Arrangement.spacedBy(2.dp),
                ) {
                    if (presentation.isCurrent) {
                        PlayingBars(presentation.animateBars)
                    } else {
                        PlayCountBadge(track.playCount)
                    }
                    Text(
                        text = formatDuration(track.durationMs),
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
            HorizontalDivider(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(start = 88.dp)
                    .align(Alignment.BottomStart),
                color = MaterialTheme.colorScheme.outlineVariant,
            )
        }
    }
}

@Composable
private fun TrackRating(rating: Int) {
    val normalizedRating = rating.coerceIn(0, 5)
    Row(verticalAlignment = Alignment.CenterVertically) {
        MaterialSymbol(
            name = if (normalizedRating > 0) "star" else "star_outline",
            contentDescription = "$normalizedRating of 5 stars",
            tint = MaterialTheme.colorScheme.tertiary,
            sizeSp = 14,
        )
        Text(
            text = "$normalizedRating/5",
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.tertiary,
        )
    }
}

@Composable
private fun PlayCountBadge(playCount: Long) {
    val normalizedPlayCount = playCount.coerceAtLeast(0)
    Surface(
        color = MaterialTheme.colorScheme.secondaryContainer,
        contentColor = MaterialTheme.colorScheme.onSecondaryContainer,
        shape = MaterialTheme.shapes.small,
    ) {
        Row(
            modifier = Modifier.padding(horizontal = 5.dp, vertical = 1.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            MaterialSymbol("play_arrow", "$normalizedPlayCount plays", sizeSp = 12)
            Text(normalizedPlayCount.toString(), style = MaterialTheme.typography.labelSmall)
        }
    }
}

private fun <T> androidx.compose.foundation.lazy.LazyListScope.windowContinuation(
    window: LibraryWindow<T>,
    lastRequestedOffset: Long?,
    loadMore: (LibraryWindowRange) -> Unit,
) {
    val request = window.nextRequest(lastRequestedOffset) ?: return
    item(key = "load-window-${request.offset}") {
        LaunchedEffect(request.offset) { loadMore(request) }
        Text(
            text = "Loading…",
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.padding(16.dp),
        )
    }
}

private fun LibraryTrack.details(): String =
    listOf(artist, album).filter(String::isNotBlank).joinToString(" • ").ifBlank {
        "Unknown artist"
    }

private fun LibraryAlbum.details(): String = buildList {
    add(artist.ifBlank { "Unknown artist" })
    year?.let { add(it.toString()) }
    add("$trackCount tracks")
}.joinToString(" • ")

private fun LibraryArtist.details(): String = "$albumCount albums • $trackCount tracks"

internal fun formatDuration(durationMs: Long): String {
    val totalSeconds = durationMs.coerceAtLeast(0) / 1_000
    return "%d:%02d".format(totalSeconds / 60, totalSeconds % 60)
}

private fun Throwable.browseDetail(action: String): String =
    "Could not $action: ${message ?: javaClass.simpleName}"
