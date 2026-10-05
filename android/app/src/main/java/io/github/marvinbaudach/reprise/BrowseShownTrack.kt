package io.github.marvinbaudach.reprise

import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue

/**
 * The row behind the mini player and the sheet, and whether it still answers
 * for what is playing. [track] is null when nothing is shown.
 */
internal class ShownTrack(
    val playingTrackId: Long?,
    val track: LibraryTrack?,
    val isStale: Boolean,
)

/**
 * The row behind the mini player and the sheet is database I/O, so it is
 * asked for from an effect and answered later, never fetched inside the
 * composition. Reads no longer wait for a folder scan, but they still do
 * not belong on the main thread. See [TrackLoader].
 */
@Composable
internal fun rememberShownTrack(
    playback: LibraryPlayback,
    surfaceState: MobileSurfaceViewModel,
    loadTrack: (Long, (LibraryTrack?) -> Unit) -> Unit,
): ShownTrack {
    val trackAnalysis = LocalTrackAnalysis.current
    val playbackControls = LocalPlaybackControls.current
    val trackArtwork = LocalTrackArtwork.current
    var answeredTrack by remember { mutableStateOf<AnsweredTrack?>(null) }
    val playingTrackId = playback.currentTrackId
    SideEffect { surfaceState.observePlayingTrack(playingTrackId) }
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
    return ShownTrack(
        playingTrackId = playingTrackId,
        track = if (playingTrackId == null) null else lastAnsweredTrack?.track,
        isStale = lastAnsweredTrack != null && lastAnsweredTrack.id != playingTrackId,
    )
}
