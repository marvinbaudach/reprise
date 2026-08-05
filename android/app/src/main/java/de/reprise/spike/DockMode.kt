package de.reprise.spike

import android.text.format.DateFormat
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import de.reprise.spike.ui.theme.AmbientTrueBlack
import java.util.Date
import kotlinx.coroutines.delay
import uniffi.reprise_android_ffi.AndroidArtworkSize

private const val DOCK_COVER_DP = 290
private const val DOCK_PLAY_DP = 96
private const val DOCK_SKIP_DP = 76
private const val DOCK_STAR_DP = 64

@Composable
internal fun DockModeSurface(
    track: LibraryTrack,
    playback: PlaybackUiState,
    surfaceState: MobileSurfaceViewModel,
) {
    val visual = rememberTrackArtworkVisual(track.uri, AndroidArtworkSize.NOW_PLAYING)
    LaunchedEffect(track.id) { surfaceState.observeDockTrack(track.id) }
    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(AmbientTrueBlack)
            .testTag("dock-surface"),
    ) {
        AmbientFields(visual?.ambientColors)
        Row(
            modifier = Modifier
                .fillMaxSize()
                .padding(horizontal = 16.dp, vertical = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Box(
                modifier = Modifier
                    .width(DOCK_COVER_DP.dp)
                    .fillMaxHeight(),
                contentAlignment = Alignment.Center,
            ) {
                ArtworkCover(
                    visual = visual,
                    size = DOCK_COVER_DP,
                    shape = RoundedCornerShape(28.dp),
                    modifier = Modifier.testTag("dock-cover"),
                )
            }
            Spacer(Modifier.width(24.dp))
            Column(
                modifier = Modifier
                    .weight(1f)
                    .fillMaxHeight(),
            ) {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.End,
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(
                        text = rememberDockClock(),
                        style = MaterialTheme.typography.titleLarge,
                        modifier = Modifier.testTag("dock-clock"),
                    )
                    IconButton(
                        onClick = surfaceState::exitDockMode,
                        modifier = Modifier.size(48.dp),
                    ) {
                        MaterialSymbol("close", "Exit dock mode", sizeSp = 30)
                    }
                }
                Text(
                    text = track.title,
                    style = TextStyle(
                        fontSize = 46.sp,
                        lineHeight = 52.sp,
                        fontWeight = FontWeight.SemiBold,
                    ),
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.testTag("dock-title"),
                )
                Text(
                    text = track.artist.ifBlank { "Unknown artist" },
                    style = TextStyle(fontSize = 22.sp, lineHeight = 28.sp),
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Spacer(Modifier.weight(1f))
                DockTransport(track, playback, surfaceState)
            }
        }
    }
}

@Composable
internal fun DockModeWaitingSurface() {
    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(AmbientTrueBlack)
            .testTag("dock-surface"),
    ) {
        AmbientFields(null)
    }
}

@Composable
private fun DockTransport(
    track: LibraryTrack,
    playback: PlaybackUiState,
    surfaceState: MobileSurfaceViewModel,
) {
    val controls = LocalPlaybackControls.current
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        DockTransportButton("skip_previous", "Previous track", DOCK_SKIP_DP, "dock-previous") {
            controls.previous()
        }
        DockTransportButton(
            symbol = if (playback.isPlaying) "pause" else "play_arrow",
            description = playback.playPauseLabel,
            size = DOCK_PLAY_DP,
            tag = "dock-play",
            primary = true,
            onClick = controls::togglePause,
        )
        DockTransportButton("skip_next", "Next track", DOCK_SKIP_DP, "dock-next") {
            controls.next()
        }
        DockStar(track, surfaceState)
    }
}

@Composable
private fun DockTransportButton(
    symbol: String,
    description: String,
    size: Int,
    tag: String,
    primary: Boolean = false,
    onClick: () -> Unit,
) {
    IconButton(
        onClick = onClick,
        modifier = Modifier
            .size(size.dp)
            .testTag(tag)
            .then(
                if (primary) {
                    Modifier
                        .clip(MaterialTheme.shapes.extraLarge)
                        .background(MaterialTheme.colorScheme.primary)
                } else {
                    Modifier
                },
            ),
    ) {
        MaterialSymbol(
            symbol,
            description,
            tint = if (primary) {
                MaterialTheme.colorScheme.onPrimary
            } else {
                MaterialTheme.colorScheme.onSurface
            },
            sizeSp = if (primary) 56 else 44,
        )
    }
}

/**
 * The dock's one star, reading the rating from the same place the sheet and the
 * library row read it — see [MobileSurfaceViewModel.ratingOf]. It keeps no copy
 * of its own; a copy here is what used to survive the ✕ and leave the sheet
 * showing the rating from before the dock was entered.
 */
@Composable
private fun DockStar(track: LibraryTrack, surfaceState: MobileSurfaceViewModel) {
    val setRating = LocalPlaybackControls.current::setRating
    val rating = surfaceState.ratingOf(track)
    var failure by remember(track.id) { mutableStateOf<TransientMessage?>(null) }
    IconButton(
        onClick = {
            val target = surfaceState.dockRatingTarget(track.id, rating)
            if (target == rating) return@IconButton
            setRating(track.id, target) { message ->
                if (message == null) {
                    surfaceState.confirmRating(track.id, rating, target)
                    failure = null
                } else {
                    failure = TransientMessage(message).after(failure)
                }
            }
        },
        modifier = Modifier
            .size(DOCK_STAR_DP.dp)
            .testTag("dock-star")
            .semantics { stateDescription = "Rating $rating of 5" },
    ) {
        MaterialSymbol(
            name = "star",
            contentDescription = if (rating == 5) {
                "Restore previous rating"
            } else {
                "Rate five stars"
            },
            tint = MaterialTheme.colorScheme.tertiary,
            sizeSp = 48,
            filled = rating == 5,
        )
    }
    TransientMessageText(failure) { failure = null }
}

@Composable
private fun rememberDockClock(): String {
    val context = LocalContext.current
    val clock by produceState(initialValue = DateFormat.getTimeFormat(context).format(Date())) {
        while (true) {
            delay(60_000)
            value = DateFormat.getTimeFormat(context).format(Date())
        }
    }
    return clock
}
