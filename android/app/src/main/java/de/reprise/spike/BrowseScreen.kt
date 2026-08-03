package de.reprise.spike

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.material3.Button
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.PrimaryTabRow
import androidx.compose.material3.Tab
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp

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
    searchTitles: (String) -> List<LibraryTrack>,
    openAlbum: (LibraryAlbum) -> AlbumTrackList,
    playTracks: (PlaybackSelection, (String) -> Unit) -> Unit,
    togglePause: () -> Unit,
    next: () -> Unit,
    previous: () -> Unit,
) {
    var selectedTab by remember { mutableStateOf(BrowseTab.TITLES) }
    var searchText by remember(state) { mutableStateOf("") }
    var visibleTitles by remember(state) { mutableStateOf(state.titles) }
    var selectedAlbum by remember(state) { mutableStateOf<AlbumTrackList?>(null) }
    var browseError by remember(state) { mutableStateOf(state.message) }

    fun play(selection: PlaybackSelection) {
        browseError = null
        playTracks(selection) { message -> browseError = message }
    }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = 16.dp, vertical = 20.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            Button(onClick = rescan) {
                Text("Rescan")
            }
            Button(onClick = chooseFolder) {
                Text("Choose another folder")
            }
        }
        browseError?.let { Text(it, color = MaterialTheme.colorScheme.error) }
        PlaybackControls(
            state = playback,
            togglePause = togglePause,
            next = next,
            previous = previous,
        )
        PrimaryTabRow(selectedTabIndex = selectedTab.ordinal) {
            BrowseTab.entries.forEach { tab ->
                Tab(
                    selected = selectedTab == tab,
                    onClick = { selectedTab = tab },
                    text = { Text(tab.label) },
                )
            }
        }
        when (selectedTab) {
            BrowseTab.TITLES -> TitlesTab(
                tracks = visibleTitles,
                searchText = searchText,
                search = { text ->
                    searchText = text
                    runCatching { searchTitles(text) }
                        .onSuccess { tracks ->
                            visibleTitles = tracks
                            browseError = null
                        }
                        .onFailure { error -> browseError = error.browseDetail("search") }
                },
                play = { index -> play(PlaybackSelection(visibleTitles, index)) },
            )
            BrowseTab.ALBUMS -> AlbumsTab(
                albums = state.albums,
                selectedAlbum = selectedAlbum,
                openAlbum = { album ->
                    runCatching { openAlbum(album) }
                        .onSuccess { detail ->
                            selectedAlbum = detail
                            browseError = null
                        }
                        .onFailure { error -> browseError = error.browseDetail("open the album") }
                },
                closeAlbum = { selectedAlbum = null },
                play = { index -> selectedAlbum?.let { play(it.playbackSelection(index)) } },
            )
            BrowseTab.ARTISTS -> ArtistsTab(state.artists)
        }
    }
}

@Composable
private fun TitlesTab(
    tracks: List<LibraryTrack>,
    searchText: String,
    search: (String) -> Unit,
    play: (Int) -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        OutlinedTextField(
            value = searchText,
            onValueChange = search,
            label = { Text("Search titles") },
            singleLine = true,
            modifier = Modifier.fillMaxWidth(),
        )
        if (tracks.isEmpty()) {
            Text(if (searchText.isEmpty()) "No tracks found in this folder." else "No matches.")
        } else {
            TrackRows(tracks = tracks, play = play)
        }
    }
}

@Composable
private fun AlbumsTab(
    albums: List<LibraryAlbum>,
    selectedAlbum: AlbumTrackList?,
    openAlbum: (LibraryAlbum) -> Unit,
    closeAlbum: () -> Unit,
    play: (Int) -> Unit,
) {
    if (selectedAlbum != null) {
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Button(onClick = closeAlbum) {
                Text("Back to albums")
            }
            Text(selectedAlbum.album.title, style = MaterialTheme.typography.titleLarge)
            Text(selectedAlbum.album.artist)
            if (selectedAlbum.tracks.isEmpty()) {
                Text("No tracks in this album.")
            } else {
                TrackRows(tracks = selectedAlbum.tracks, play = play)
            }
        }
        return
    }

    if (albums.isEmpty()) {
        Text("No albums found in this folder.")
        return
    }
    LazyColumn(modifier = Modifier.fillMaxSize()) {
        items(albums, key = { album -> "${album.artist}\u0000${album.title}" }) { album ->
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
    }
}

@Composable
private fun ArtistsTab(artists: List<LibraryArtist>) {
    if (artists.isEmpty()) {
        Text("No artists found in this folder.")
        return
    }
    LazyColumn(modifier = Modifier.fillMaxSize()) {
        items(artists, key = LibraryArtist::name) { artist ->
            ListItem(
                headlineContent = { Text(artist.name) },
                supportingContent = { Text(artist.details()) },
            )
            HorizontalDivider()
        }
    }
}

@Composable
private fun TrackRows(tracks: List<LibraryTrack>, play: (Int) -> Unit) {
    LazyColumn(modifier = Modifier.fillMaxSize()) {
        itemsIndexed(tracks, key = { _, track -> track.uri }) { index, track ->
            ListItem(
                headlineContent = { Text(track.title) },
                supportingContent = { Text(track.details()) },
                trailingContent = { Text(formatDuration(track.durationMs)) },
                modifier = Modifier
                    .fillMaxWidth()
                    .clickable { play(index) },
            )
            HorizontalDivider()
        }
    }
}

@Composable
private fun PlaybackControls(
    state: PlaybackUiState,
    togglePause: () -> Unit,
    next: () -> Unit,
    previous: () -> Unit,
) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Button(onClick = previous, enabled = state.ready && state.currentIndex != null) {
            Text("Previous")
        }
        Button(onClick = togglePause, enabled = state.ready && state.currentIndex != null) {
            Text(state.playPauseLabel)
        }
        Button(onClick = next, enabled = state.ready && state.currentIndex != null) {
            Text("Next")
        }
        Text(state.positionReadout)
    }
    state.error?.let { Text(it, color = MaterialTheme.colorScheme.error) }
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
    val totalSeconds = (durationMs.coerceAtLeast(0) / 1_000)
    return "%d:%02d".format(totalSeconds / 60, totalSeconds % 60)
}

private fun Throwable.browseDetail(action: String): String =
    "Could not $action: ${message ?: javaClass.simpleName}"
