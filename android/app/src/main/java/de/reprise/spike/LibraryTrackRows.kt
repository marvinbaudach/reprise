package de.reprise.spike

import androidx.compose.foundation.clickable
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
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.GridItemSpan
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.platform.testTag

internal data class LibraryRatingControl(
    val enabled: Boolean,
    val select: (Boolean) -> Unit,
)

/** The shipped activity supplies the persisted value; isolated previews retain the old row. */
internal val LocalLibraryRatingControl = staticCompositionLocalOf {
    LibraryRatingControl(enabled = true, select = {})
}

/**
 * The library's track list: the 72 dp rows, their continuation sentinel, and
 * the badges the row carries. Shared by the Titles tab and by an opened album,
 * which is why it is not part of either.
 */
@Composable
internal fun TrackRows(
    surfaceLayout: SurfaceLayout,
    surfaceState: MobileSurfaceViewModel,
    listKey: LibraryListKey,
    tracks: LibraryWindow<LibraryTrack>,
    playback: PlaybackUiState,
    lastRequestedOffset: Long?,
    play: (Int) -> Unit,
    loadMore: (LibraryWindowRange) -> Unit,
) {
    val metrics = libraryFrameMetrics(surfaceLayout)
    val content = trackListContent(tracks, lastRequestedOffset)
    val anchor = surfaceState.scrollPosition(listKey).within(content.size)
    if (surfaceLayout == SurfaceLayout.WIDE_SHORT) {
        val gridState = rememberLibraryGridState(anchor)
        ObserveLibraryGridAnchor(listKey, gridState, surfaceState)
        LazyVerticalGrid(
            columns = GridCells.Fixed(metrics.listColumns),
            state = gridState,
            modifier = Modifier
                .fillMaxSize()
                .testTag(listKey.testTag()),
            horizontalArrangement = Arrangement.spacedBy(metrics.listColumnGapDp.dp),
        ) {
            items(
                items = content,
                key = TrackListContent::stableKey,
                span = { item ->
                    if (item is TrackListContent.Continuation) {
                        GridItemSpan(maxLineSpan)
                    } else {
                        GridItemSpan(1)
                    }
                },
            ) { item ->
                TrackListItem(item, surfaceState, metrics, playback, play, loadMore)
            }
        }
        return
    }

    val listState = rememberLibraryListState(anchor)
    ObserveLibraryListAnchor(listKey, listState, surfaceState)
    LazyColumn(
        state = listState,
        modifier = Modifier
            .fillMaxSize()
            .testTag(listKey.testTag()),
    ) {
        items(
            items = content,
            key = TrackListContent::stableKey,
        ) { content ->
            TrackListItem(content, surfaceState, metrics, playback, play, loadMore)
        }
    }
}

private fun TrackListContent.stableKey(): String = when (this) {
    is TrackListContent.Row -> "track-${track.uri}"
    is TrackListContent.Continuation -> "load-window-${request.offset}"
}

private fun LibraryListKey.testTag(): String = when (this) {
    LibraryListKey.TITLES -> "library-titles-list"
    LibraryListKey.ALBUMS -> "library-albums-list"
    LibraryListKey.ARTISTS -> "library-artists-list"
    LibraryListKey.ALBUM_TRACKS -> "library-album-tracks-list"
}

@Composable
private fun TrackListItem(
    content: TrackListContent,
    surfaceState: MobileSurfaceViewModel,
    metrics: LibraryFrameMetrics,
    playback: PlaybackUiState,
    play: (Int) -> Unit,
    loadMore: (LibraryWindowRange) -> Unit,
) {
    when (content) {
        is TrackListContent.Row -> LibraryTrackRow(
            track = content.track,
            // The row a window was paged in with is a *copy* of the track, and
            // rating it in Now Playing does not rewrite that copy. The rating is
            // therefore read from the one place all three surfaces read it —
            // only for the rows on screen, never for the whole window.
            rating = surfaceState.ratingOf(content.track),
            presentation = content.track.playbackPresentation(playback),
            metrics = metrics,
            play = { play(content.index) },
        )
        is TrackListContent.Continuation -> {
            LaunchedEffect(content.request.offset) { loadMore(content.request) }
            LoadingWindowRow()
        }
    }
}

@Composable
private fun LibraryTrackRow(
    track: LibraryTrack,
    rating: Int,
    presentation: TrackPlaybackPresentation,
    metrics: LibraryFrameMetrics,
    play: () -> Unit,
) {
    Surface(
        modifier = Modifier
            .fillMaxWidth()
            .height(metrics.trackRowHeightDp.dp)
            .testTag("library-track-row-${track.id}")
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
                    size = metrics.trackCoverSizeDp,
                    // The row is one clickable node, so anything described
                    // below it is merged into what the row announces. A cover
                    // saying "Album artwork" there replaces the song.
                    decorative = true,
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
                        if (LocalLibraryRatingControl.current.enabled) {
                            TrackRating(rating)
                        }
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
            name = "star",
            contentDescription = "$normalizedRating of 5 stars",
            tint = MaterialTheme.colorScheme.tertiary,
            sizeSp = 14,
            filled = normalizedRating > 0,
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

/** What a window's continuation sentinel looks like while it loads. */
@Composable
internal fun LoadingWindowRow() {
    Text(
        text = "Loading…",
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        modifier = Modifier.padding(16.dp),
    )
}
