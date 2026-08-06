package de.reprise.spike

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
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
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp

@Composable
internal fun TitlesTab(
    surfaceLayout: SurfaceLayout,
    surfaceState: MobileSurfaceViewModel,
    tracks: LibraryWindow<LibraryTrack>,
    searchVisible: Boolean,
    searchText: String,
    search: (String) -> Unit,
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
internal fun AlbumsTab(
    surfaceLayout: SurfaceLayout,
    surfaceState: MobileSurfaceViewModel,
    albums: LibraryWindow<LibraryAlbum>,
    selectedAlbum: AlbumTrackList?,
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
        AlbumRows(
            surfaceLayout = surfaceLayout,
            surfaceState = surfaceState,
            albums = albums,
            requestedOffset = albumsRequestedOffset,
            openAlbum = openAlbum,
            loadMore = loadMoreAlbums,
        )
    }
}

@Composable
internal fun ArtistsTab(
    surfaceLayout: SurfaceLayout,
    surfaceState: MobileSurfaceViewModel,
    artists: LibraryWindow<LibraryArtist>,
    selectedArtist: ArtistTrackList?,
    playback: PlaybackUiState,
    openArtist: (LibraryArtist) -> Unit,
    closeArtist: () -> Unit,
    play: (Int) -> Unit,
    lastRequestedOffset: Long?,
    artistRequestedOffset: Long?,
    loadMoreArtists: (LibraryWindowRange) -> Unit,
    loadMoreArtistTracks: (LibraryWindowRange) -> Unit,
) {
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
            Text(
                selectedArtist.tracks.visibleCountLabel("track", "tracks"),
                style = MaterialTheme.typography.labelMedium,
                modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp),
            )
            if (selectedArtist.tracks.rows.isEmpty()) {
                Text("No tracks by this artist.", modifier = Modifier.padding(16.dp))
            } else {
                ListPlayButton(
                    description = "Play ${selectedArtist.artist.name}",
                    onClick = { play(0) },
                )
                TrackRows(
                    surfaceLayout = surfaceLayout,
                    surfaceState = surfaceState,
                    listKey = LibraryListKey.ARTIST_TRACKS,
                    tracks = selectedArtist.tracks,
                    playback = playback,
                    lastRequestedOffset = artistRequestedOffset,
                    play = play,
                    loadMore = loadMoreArtistTracks,
                    subtitle = TrackRowSubtitle.ALBUM_ONLY,
                )
            }
        }
        return
    }

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
internal fun FavouritesTab(
    surfaceLayout: SurfaceLayout,
    surfaceState: MobileSurfaceViewModel,
    tracks: LibraryWindow<LibraryTrack>,
    playback: PlaybackUiState,
    lastRequestedOffset: Long?,
    play: (Int) -> Unit,
    loadMore: (LibraryWindowRange) -> Unit,
    removeFavourite: (LibraryTrack) -> Unit,
) {
    Column(modifier = Modifier.fillMaxSize()) {
        Text(
            tracks.visibleCountLabel("favourite", "favourites"),
            style = MaterialTheme.typography.labelMedium,
            modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp),
        )
        if (tracks.rows.isEmpty()) {
            Text("No favourites yet.", modifier = Modifier.padding(16.dp))
        } else {
            ListPlayButton(description = "Play favourites", onClick = { play(0) })
            TrackRows(
                surfaceLayout = surfaceLayout,
                surfaceState = surfaceState,
                listKey = LibraryListKey.FAVOURITES,
                tracks = tracks,
                playback = playback,
                lastRequestedOffset = lastRequestedOffset,
                play = play,
                loadMore = loadMore,
                onFavouriteChanged = { track, favourite ->
                    if (!favourite) removeFavourite(track)
                },
            )
        }
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
) {
    val key = LibraryListKey.ALBUMS
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
            modifier = Modifier.fillMaxSize().testTag("library-albums-list"),
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
        modifier = Modifier.fillMaxSize().testTag("library-albums-list"),
    ) {
        items(albums.rows, key = { album -> "${album.artist}\u0000${album.title}" }) { album ->
            AlbumRow(album, openAlbum)
        }
        windowContinuation(albums, requestedOffset, loadMore)
    }
}

@Composable
private fun AlbumRow(album: LibraryAlbum, openAlbum: (LibraryAlbum) -> Unit) {
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
