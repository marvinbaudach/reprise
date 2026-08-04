package de.reprise.spike

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp

@Composable
internal fun TitlesTab(
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
internal fun AlbumsTab(
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
internal fun ArtistsTab(
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
