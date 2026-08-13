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
    albumRequestedOffset: Long?,
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
                lastRequestedOffset = albumRequestedOffset,
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
    albumRequestedOffset: Long? = null,
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
            albumRequestedOffset = albumRequestedOffset,
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
                ArtistDetailSections(
                    surfaceLayout = surfaceLayout,
                    surfaceState = surfaceState,
                    albums = selectedArtist.albums,
                    untaggedTracks = selectedArtist.untaggedTracks,
                    playback = playback,
                    albumsRequestedOffset = artistAlbumsRequestedOffset,
                    tracksRequestedOffset = artistRequestedOffset,
                    openAlbum = openAlbum,
                    play = play,
                    loadMoreAlbums = loadMoreArtistAlbums,
                    loadMoreTracks = loadMoreArtistTracks,
                )
            } else if (hasOtherTitles) {
                ArtistDetailSections(
                    surfaceLayout = surfaceLayout,
                    surfaceState = surfaceState,
                    albums = LibraryWindow.empty(),
                    untaggedTracks = selectedArtist.untaggedTracks,
                    playback = playback,
                    albumsRequestedOffset = artistAlbumsRequestedOffset,
                    tracksRequestedOffset = artistRequestedOffset,
                    openAlbum = openAlbum,
                    play = play,
                    loadMoreAlbums = loadMoreArtistAlbums,
                    loadMoreTracks = loadMoreArtistTracks,
                )
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
        ArtistSearchSections(
            surfaceState = surfaceState,
            albums = albumResults,
            artists = artists,
            albumsRequestedOffset = artistAlbumsRequestedOffset,
            artistsRequestedOffset = lastRequestedOffset,
            openAlbum = openAlbum,
            openArtist = openArtist,
            loadMoreAlbums = loadMoreArtistAlbums,
            loadMoreArtists = loadMoreArtists,
        )
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
private fun ArtistDetailSections(
    surfaceLayout: SurfaceLayout,
    surfaceState: MobileSurfaceViewModel,
    albums: LibraryWindow<LibraryAlbum>,
    untaggedTracks: LibraryWindow<LibraryTrack>,
    playback: PlaybackUiState,
    albumsRequestedOffset: Long?,
    tracksRequestedOffset: Long?,
    openAlbum: (LibraryAlbum) -> Unit,
    play: (Int) -> Unit,
    loadMoreAlbums: (LibraryWindowRange) -> Unit,
    loadMoreTracks: (LibraryWindowRange) -> Unit,
) {
    val key = LibraryListKey.ARTIST_ALBUMS
    val trackContent = trackListContent(untaggedTracks, tracksRequestedOffset)
    val albumContinuation = albums.nextRequest(albumsRequestedOffset)
    val albumItemCount = albums.rows.size + if (albums.rows.isEmpty()) 0 else 1
    val itemCount = albumItemCount + trackContent.size +
        (if (albumContinuation == null) 0 else 1) +
        (if (untaggedTracks.rows.isEmpty()) 0 else 1)
    val anchor = surfaceState.scrollPosition(key).within(itemCount)
    val listState = rememberLibraryListState(anchor)
    ObserveLibraryListAnchor(key, listState, surfaceState)
    val metrics = libraryFrameMetrics(surfaceLayout)
    LazyColumn(
        state = listState,
        modifier = Modifier.fillMaxSize().testTag(key.testTag()),
    ) {
        if (albums.rows.isNotEmpty()) {
            item(key = "artist-albums-heading") { SectionHeading("Albums") }
            items(
                albums.rows,
                key = { album -> "artist-album-${album.artist}\u0000${album.title}" },
            ) { album -> AlbumRow(album, openAlbum) }
            albumContinuation?.let { request ->
                item(key = "artist-albums-load-${request.offset}") {
                    LaunchedEffect(request.offset) { loadMoreAlbums(request) }
                    LoadingWindowRow()
                }
            }
        }
        if (untaggedTracks.rows.isNotEmpty()) {
            item(key = "artist-tracks-heading") { SectionHeading("Other titles") }
            items(
                trackContent,
                key = { content ->
                    when (content) {
                        is TrackListContent.Row -> "artist-track-${content.track.uri}"
                        is TrackListContent.Continuation ->
                            "artist-tracks-load-${content.request.offset}"
                    }
                },
            ) { content ->
                TrackListItem(
                    content = content,
                    surfaceState = surfaceState,
                    metrics = metrics,
                    playback = playback,
                    play = play,
                    loadMore = loadMoreTracks,
                    subtitle = TrackRowSubtitle.ALBUM_ONLY,
                    onFavouriteChanged = { _, _ -> },
                    queueActions = null,
                    rowCount = untaggedTracks.rows.size,
                )
            }
        }
    }
}

@Composable
private fun ArtistSearchSections(
    surfaceState: MobileSurfaceViewModel,
    albums: LibraryWindow<LibraryAlbum>,
    artists: LibraryWindow<LibraryArtist>,
    albumsRequestedOffset: Long?,
    artistsRequestedOffset: Long?,
    openAlbum: (LibraryAlbum) -> Unit,
    openArtist: (LibraryArtist) -> Unit,
    loadMoreAlbums: (LibraryWindowRange) -> Unit,
    loadMoreArtists: (LibraryWindowRange) -> Unit,
) {
    val key = LibraryListKey.ARTIST_SEARCH_ALBUMS
    val albumContinuation = albums.nextRequest(albumsRequestedOffset)
    val artistContinuation = artists.nextRequest(artistsRequestedOffset)
    val albumItemCount = albums.rows.size + if (albums.rows.isEmpty()) 0 else 1
    val artistItemCount = artists.rows.size + if (artists.rows.isEmpty()) 0 else 1
    val itemCount = albumItemCount + artistItemCount +
        (if (albumContinuation == null) 0 else 1) +
        (if (artistContinuation == null) 0 else 1)
    val anchor = surfaceState.scrollPosition(key).within(itemCount)
    val listState = rememberLibraryListState(anchor)
    ObserveLibraryListAnchor(key, listState, surfaceState)
    LazyColumn(
        state = listState,
        modifier = Modifier.fillMaxSize().testTag(key.testTag()),
    ) {
        if (albums.rows.isNotEmpty()) {
            item(key = "search-albums-heading") { SectionHeading("Albums") }
            items(
                albums.rows,
                key = { album -> "search-album-${album.artist}\u0000${album.title}" },
            ) { album -> AlbumRow(album, openAlbum) }
            albumContinuation?.let { request ->
                item(key = "search-albums-load-${request.offset}") {
                    LaunchedEffect(request.offset) { loadMoreAlbums(request) }
                    LoadingWindowRow()
                }
            }
        }
        if (artists.rows.isNotEmpty()) {
            item(key = "search-artists-heading") { SectionHeading("Artists") }
            items(artists.rows, key = { artist -> "search-artist-${artist.name}" }) { artist ->
                ArtistRow(artist, openArtist)
            }
            artistContinuation?.let { request ->
                item(key = "search-artists-load-${request.offset}") {
                    LaunchedEffect(request.offset) { loadMoreArtists(request) }
                    LoadingWindowRow()
                }
            }
        }
    }
}

@Composable
private fun SectionHeading(text: String) {
    Text(
        text,
        style = MaterialTheme.typography.titleMedium,
        modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
    )
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
