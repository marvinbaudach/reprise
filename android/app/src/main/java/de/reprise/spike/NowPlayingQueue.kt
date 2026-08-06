package de.reprise.spike

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp

/** The future-only page inside Now Playing. */
@Composable
internal fun NowPlayingQueuePage(
    playback: PlaybackUiState,
    surfaceState: MobileSurfaceViewModel,
) {
    val controls = LocalPlaybackControls.current
    var tracks by remember { mutableStateOf<LibraryWindow<LibraryTrack>?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    var requestedOffset by remember { mutableStateOf<Long?>(null) }
    var generation by remember { mutableStateOf(0L) }

    fun load(window: LibraryWindowRange, append: Boolean) {
        val requestedGeneration = generation
        controls.loadUpcomingTracks(window) { outcome ->
            if (requestedGeneration != generation) return@loadUpcomingTracks
            outcome.onSuccess { answer ->
                tracks = if (append) tracks?.append(answer) ?: answer else answer
                requestedOffset = null
                error = null
            }.onFailure { failure ->
                error = "Could not load the queue: ${failure.message ?: "unknown error"}"
            }
        }
    }

    fun reload() {
        generation += 1
        requestedOffset = null
        load(firstLibraryWindow(), append = false)
    }

    fun edit(action: ((Result<Boolean>) -> Unit) -> Unit) {
        action { outcome ->
            outcome.onSuccess {
                // `false` is the stale-view answer, and therefore needs this
                // reload every bit as much as a successful edit does.
                reload()
            }.onFailure { failure ->
                error = "Could not edit the queue: ${failure.message ?: "unknown error"}"
            }
        }
    }

    LaunchedEffect(playback.currentTrackId) {
        load(firstLibraryWindow(), append = false)
    }

    Box(modifier = Modifier.fillMaxSize()) {
        when {
            error != null -> Text(
                text = checkNotNull(error),
                color = MaterialTheme.colorScheme.error,
                modifier = Modifier.padding(16.dp),
            )
            tracks == null -> Text(
                text = "Loading…",
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.padding(16.dp),
            )
            tracks?.rows?.isEmpty() == true -> Text(
                text = "The queue is exhausted.",
                modifier = Modifier.align(Alignment.Center).padding(16.dp),
            )
            else -> Column(modifier = Modifier.fillMaxSize()) {
                Text(
                    text = checkNotNull(tracks).visibleCountLabel(
                        "upcoming track",
                        "upcoming tracks",
                    ),
                    style = MaterialTheme.typography.labelMedium,
                    modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp),
                )
                Box(modifier = Modifier.weight(1f)) {
                    TrackRows(
                        surfaceLayout = SurfaceLayout.STACKED,
                        surfaceState = surfaceState,
                        listKey = LibraryListKey.UPCOMING,
                        tracks = checkNotNull(tracks),
                        playback = playback,
                        lastRequestedOffset = requestedOffset,
                        play = {},
                        loadMore = { request ->
                            if (tracks?.nextRequest(requestedOffset) == request) {
                                requestedOffset = request.offset
                                load(request, append = true)
                            }
                        },
                        queueActions = QueueRowActions(
                            play = { position, trackId ->
                                edit { report ->
                                    controls.playUpcomingTrackNow(position, trackId, report)
                                }
                            },
                            move = { from, trackId, to ->
                                edit { report ->
                                    controls.moveUpcomingTrack(from, trackId, to, report)
                                }
                            },
                            remove = { position, trackId ->
                                edit { report ->
                                    controls.removeUpcomingTrack(position, trackId, report)
                                }
                            },
                        ),
                    )
                }
            }
        }
    }
}
