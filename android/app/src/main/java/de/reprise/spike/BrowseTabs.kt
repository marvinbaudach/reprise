package de.reprise.spike

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.GridItemSpan
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items as gridItems
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Button
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp

internal fun BrowseTab.emptyMessage(searchText: String): String = if (searchText.isNotBlank()) {
    "No matching ${label.lowercase()}."
} else {
    when (this) {
        BrowseTab.TITLES -> "No tracks found in this folder."
        BrowseTab.ARTISTS -> "No artists found in this folder."
        // Unreachable in practice — the queue page speaks for itself.
        BrowseTab.QUEUE -> "The queue is exhausted."
    }
}

@Composable
internal fun TitlesTab(
    surfaceLayout: SurfaceLayout,
    surfaceState: MobileSurfaceViewModel,
    tracks: LibraryWindow<LibraryTrack>,
    searchText: String,
    playback: PlaybackUiState,
    lastRequestedOffset: Long?,
    play: (Int) -> Unit,
    loadMore: (LibraryWindowRange) -> Unit,
) {
    Column(modifier = Modifier.fillMaxSize()) {
        if (tracks.rows.isEmpty()) {
            Text(
                text = BrowseTab.TITLES.emptyMessage(searchText),
                modifier = Modifier.padding(16.dp),
            )
        } else {
            TrackRows(
                surfaceLayout = surfaceLayout,
                surfaceState = surfaceState,
                listKey = LibraryListKey.TITLES,
                tracks = tracks,
                playback = playback,
                lastRequestedOffset = lastRequestedOffset,
                play = play,
                loadMore = loadMore,
            )
        }
    }
}

@Composable
internal fun LibrarySearchField(
    tab: BrowseTab,
    searchText: String,
    search: (String) -> Unit,
    close: () -> Unit,
) {
    val focusRequester = remember { FocusRequester() }
    val keyboard = LocalSoftwareKeyboardController.current
    LaunchedEffect(Unit) {
        focusRequester.requestFocus()
        keyboard?.show()
    }
    BackHandler(onBack = close)
    OutlinedTextField(
        value = searchText,
        onValueChange = search,
        label = {
            Text(
                if (tab == BrowseTab.ARTISTS) {
                    "Search albums and artists"
                } else {
                    "Search ${tab.label.lowercase()}"
                },
            )
        },
        leadingIcon = { MaterialSymbol("search", "") },
        trailingIcon = {
            IconButton(onClick = { if (searchText.isEmpty()) close() else search("") }) {
                MaterialSymbol(
                    name = if (searchText.isEmpty()) "close" else "clear",
                    contentDescription = if (searchText.isEmpty()) {
                        "Close search"
                    } else {
                        "Clear search"
                    },
                )
            }
        },
        singleLine = true,
        modifier = Modifier
            .fillMaxWidth()
            .focusRequester(focusRequester)
            .padding(horizontal = 16.dp, vertical = 4.dp),
    )
}

@Composable
internal fun AlbumDetailPage(
    surfaceLayout: SurfaceLayout,
    surfaceState: MobileSurfaceViewModel,
    selectedAlbum: AlbumTrackList,
    playback: PlaybackUiState,
    closeAlbum: () -> Unit,
    play: (Int) -> Unit,
    loadMoreAlbumTracks: (LibraryWindowRange) -> Unit,
) {
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
        if (selectedAlbum.tracks.rows.isEmpty()) {
            Text("No tracks in this album.", modifier = Modifier.padding(16.dp))
        } else {
            ListPlayButton(
                description = "Play ${selectedAlbum.album.title}",
                onClick = { play(0) },
            )
            TrackRows(
                surfaceLayout = surfaceLayout,
                surfaceState = surfaceState,
                listKey = LibraryListKey.ALBUM_TRACKS,
                tracks = selectedAlbum.tracks,
                playback = playback,
                lastRequestedOffset = null,
                play = play,
                loadMore = loadMoreAlbumTracks,
            )
        }
    }
}

@Composable
internal fun ArtistsTab(
    surfaceLayout: SurfaceLayout,
    surfaceState: MobileSurfaceViewModel,
    artists: LibraryWindow<LibraryArtist>,
    albumResults: LibraryWindow<LibraryAlbum> = LibraryWindow.empty(),
    searchText: String,
    selectedArtist: ArtistTrackList?,
    selectedAlbum: AlbumTrackList? = null,
    playback: PlaybackUiState,
    openArtist: (LibraryArtist) -> Unit,
    openAlbum: (LibraryAlbum) -> Unit = {},
    closeArtist: () -> Unit,
    closeAlbum: () -> Unit = {},
    play: (Int) -> Unit,
    playAlbum: (Int) -> Unit = {},
    lastRequestedOffset: Long?,
    artistRequestedOffset: Long?,
    artistAlbumsRequestedOffset: Long? = null,
    loadMoreArtists: (LibraryWindowRange) -> Unit,
    loadMoreArtistTracks: (LibraryWindowRange) -> Unit,
    loadMoreArtistAlbums: (LibraryWindowRange) -> Unit = {},
    loadMoreAlbumTracks: (LibraryWindowRange) -> Unit = {},
) {
    if (selectedAlbum != null) {
        AlbumDetailPage(
            surfaceLayout = surfaceLayout,
            surfaceState = surfaceState,
            selectedAlbum = selectedAlbum,
            playback = playback,
            closeAlbum = closeAlbum,
            play = playAlbum,
            loadMoreAlbumTracks = loadMoreAlbumTracks,
        )
        return
    }
    if (selectedArtist != null) {
        Column(modifier = Modifier.fillMaxSize()) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .clickable(onClick = closeArtist)
                    .padding(horizontal = 16.dp, vertical = 8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                MaterialSymbol("arrow_back", "Back to artists")
                Text(selectedArtist.artist.name, style = MaterialTheme.typography.titleLarge)
            }
            val hasAlbums = selectedArtist.albums.rows.isNotEmpty()
            val hasOtherTitles = selectedArtist.untaggedTracks.rows.isNotEmpty()
            if (!hasAlbums && !hasOtherTitles) {
                Text("No tracks by this artist.", modifier = Modifier.padding(16.dp))
            }
            if (hasAlbums) {
                val orderedAlbums = selectedArtist.albums.copy(
                    rows = selectedArtist.albums.rows.sortedWith(
                        compareByDescending<LibraryAlbum> { it.year }
                            .thenBy(String.CASE_INSENSITIVE_ORDER) { it.title.trim() },
                    ),
                )
                Text(
                    "Albums",
                    style = MaterialTheme.typography.titleMedium,
                    modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
                )
                Box(modifier = Modifier.weight(1f)) {
                    AlbumRows(
                        surfaceLayout = surfaceLayout,
                        surfaceState = surfaceState,
                        albums = orderedAlbums,
                        requestedOffset = artistAlbumsRequestedOffset,
                        openAlbum = openAlbum,
                        loadMore = loadMoreArtistAlbums,
                        key = LibraryListKey.ARTIST_ALBUMS,
                    )
                }
            }
            if (hasOtherTitles) {
                Text(
                    "Other titles",
                    style = MaterialTheme.typography.titleMedium,
                    modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
                )
                Box(modifier = Modifier.weight(1f)) {
                    TrackRows(
                        surfaceLayout = surfaceLayout,
                        surfaceState = surfaceState,
                        listKey = LibraryListKey.ARTIST_TRACKS,
                        tracks = selectedArtist.untaggedTracks,
                        playback = playback,
                        lastRequestedOffset = artistRequestedOffset,
                        play = play,
                        loadMore = loadMoreArtistTracks,
                        subtitle = TrackRowSubtitle.ALBUM_ONLY,
                    )
                }
            }
        }
        return
    }

    if (searchText.isNotBlank()) {
        val hasAlbums = albumResults.rows.isNotEmpty()
        val hasArtists = artists.rows.isNotEmpty()
        if (!hasAlbums && !hasArtists) {
            Text("No matching albums or artists.", modifier = Modifier.padding(16.dp))
            return
        }
        Column(modifier = Modifier.fillMaxSize()) {
            if (hasAlbums) {
                Text(
                    "Albums",
                    style = MaterialTheme.typography.titleMedium,
                    modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
                )
                Box(modifier = Modifier.weight(1f)) {
                    AlbumRows(
                        surfaceLayout = surfaceLayout,
                        surfaceState = surfaceState,
                        albums = albumResults,
                        requestedOffset = artistAlbumsRequestedOffset,
                        openAlbum = openAlbum,
                        loadMore = loadMoreArtistAlbums,
                        key = LibraryListKey.ARTIST_SEARCH_ALBUMS,
                    )
                }
            }
            if (hasArtists) {
                Text(
                    "Artists",
                    style = MaterialTheme.typography.titleMedium,
                    modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
                )
                Box(modifier = Modifier.weight(1f)) {
                    ArtistRows(
                        surfaceLayout = surfaceLayout,
                        surfaceState = surfaceState,
                        artists = artists,
                        requestedOffset = lastRequestedOffset,
                        openArtist = openArtist,
                        loadMore = loadMoreArtists,
                    )
                }
            }
        }
        return
    }

    if (artists.rows.isEmpty()) {
        Text(BrowseTab.ARTISTS.emptyMessage(searchText), modifier = Modifier.padding(16.dp))
        return
    }
    Column(modifier = Modifier.fillMaxSize()) {
        ArtistRows(
            surfaceLayout = surfaceLayout,
            surfaceState = surfaceState,
            artists = artists,
            requestedOffset = lastRequestedOffset,
            openArtist = openArtist,
            loadMore = loadMoreArtists,
        )
    }
}

@Composable
private fun ListPlayButton(description: String, onClick: () -> Unit) {
    Button(
        onClick = onClick,
        modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp),
    ) {
        MaterialSymbol("play_arrow", description)
        Text("Play")
    }
}

@Composable
private fun AlbumRows(
    surfaceLayout: SurfaceLayout,
    surfaceState: MobileSurfaceViewModel,
    albums: LibraryWindow<LibraryAlbum>,
    requestedOffset: Long?,
    openAlbum: (LibraryAlbum) -> Unit,
    loadMore: (LibraryWindowRange) -> Unit,
    key: LibraryListKey,
) {
    val anchor = surfaceState.scrollPosition(key).within(albums.itemCount(requestedOffset))
    if (surfaceLayout == SurfaceLayout.WIDE_SHORT) {
        val metrics = libraryFrameMetrics(surfaceLayout)
        val gridState = rememberLibraryGridState(anchor)
        ObserveLibraryGridAnchor(key, gridState, surfaceState)
        LazyVerticalGrid(
            columns = GridCells.Fixed(2),
            state = gridState,
            horizontalArrangement = androidx.compose.foundation.layout.Arrangement.spacedBy(
                metrics.listColumnGapDp.dp,
            ),
            modifier = Modifier.fillMaxSize().testTag(key.testTag()),
        ) {
            gridItems(
                items = albums.rows,
                key = { album -> "${album.artist}\u0000${album.title}" },
            ) { album -> AlbumRow(album, openAlbum) }
            albums.nextRequest(requestedOffset)?.let { request ->
                item(key = "load-window-${request.offset}", span = { GridItemSpan(maxLineSpan) }) {
                    LaunchedEffect(request.offset) { loadMore(request) }
                    LoadingWindowRow()
                }
            }
        }
        return
    }
    val listState = rememberLibraryListState(anchor)
    ObserveLibraryListAnchor(key, listState, surfaceState)
    LazyColumn(
        state = listState,
        modifier = Modifier.fillMaxSize().testTag(key.testTag()),
    ) {
        items(albums.rows, key = { album -> "${album.artist}\u0000${album.title}" }) { album ->
            AlbumRow(album, openAlbum)
        }
        windowContinuation(albums, requestedOffset, loadMore)
    }
}

@Composable
private fun AlbumRow(album: LibraryAlbum, openAlbum: (LibraryAlbum) -> Unit) {
    val contextMenu = rememberTrackContextMenuAnchorState()
    val albumTrackIds = LocalAlbumTrackIds.current
    val controls = LocalPlaybackControls.current
    // The acknowledgement sits below the row, not inside the Box it would
    // otherwise cover — see TrackContextMenuMessage.
    Column {
        Box {
            ListItem(
                headlineContent = { Text(album.title) },
                supportingContent = { Text(album.details()) },
                trailingContent = { Text(formatDuration(album.totalDurationMs)) },
                modifier = Modifier
                    .fillMaxWidth()
                    .trackContextMenuAnchor(contextMenu) { openAlbum(album) },
            )
            TrackContextMenu(
                anchor = contextMenu,
                target = LibraryTrackMenuTarget(
                    label = album.title,
                    trackCount = album.trackCount,
                    resolveTrackIds = { albumTrackIds(album) },
                    play = { ids -> controls.playTrackIds(ids, 0) },
                ),
            )
        }
        TrackContextMenuMessage(contextMenu)
    }
    HorizontalDivider()
}

@Composable
private fun ArtistRows(
    surfaceLayout: SurfaceLayout,
    surfaceState: MobileSurfaceViewModel,
    artists: LibraryWindow<LibraryArtist>,
    requestedOffset: Long?,
    openArtist: (LibraryArtist) -> Unit,
    loadMore: (LibraryWindowRange) -> Unit,
) {
    val key = LibraryListKey.ARTISTS
    val anchor = surfaceState.scrollPosition(key).within(artists.itemCount(requestedOffset))
    if (surfaceLayout == SurfaceLayout.WIDE_SHORT) {
        val metrics = libraryFrameMetrics(surfaceLayout)
        val gridState = rememberLibraryGridState(anchor)
        ObserveLibraryGridAnchor(key, gridState, surfaceState)
        LazyVerticalGrid(
            columns = GridCells.Fixed(2),
            state = gridState,
            horizontalArrangement = androidx.compose.foundation.layout.Arrangement.spacedBy(
                metrics.listColumnGapDp.dp,
            ),
            modifier = Modifier.fillMaxSize().testTag("library-artists-list"),
        ) {
            gridItems(artists.rows, key = LibraryArtist::name) { artist ->
                ArtistRow(artist, openArtist)
            }
            artists.nextRequest(requestedOffset)?.let { request ->
                item(key = "load-window-${request.offset}", span = { GridItemSpan(maxLineSpan) }) {
                    LaunchedEffect(request.offset) { loadMore(request) }
                    LoadingWindowRow()
                }
            }
        }
        return
    }
    val listState = rememberLibraryListState(anchor)
    ObserveLibraryListAnchor(key, listState, surfaceState)
    LazyColumn(
        state = listState,
        modifier = Modifier.fillMaxSize().testTag("library-artists-list"),
    ) {
        items(artists.rows, key = LibraryArtist::name) { artist ->
            ArtistRow(artist, openArtist)
        }
        windowContinuation(artists, requestedOffset, loadMore)
    }
}

@Composable
private fun ArtistRow(artist: LibraryArtist, openArtist: (LibraryArtist) -> Unit) {
    ListItem(
        headlineContent = { Text(artist.name) },
        supportingContent = { Text(artist.details()) },
        modifier = Modifier
            .fillMaxWidth()
            .clickable { openArtist(artist) },
    )
    HorizontalDivider()
}

private fun <T> LazyListScope.windowContinuation(
    window: LibraryWindow<T>,
    lastRequestedOffset: Long?,
    loadMore: (LibraryWindowRange) -> Unit,
) {
    val request = window.nextRequest(lastRequestedOffset) ?: return
    item(key = "load-window-${request.offset}") {
        LaunchedEffect(request.offset) { loadMore(request) }
        LoadingWindowRow()
    }
}
