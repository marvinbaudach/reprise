package de.reprise.spike

import androidx.activity.compose.PredictiveBackHandler
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.IconButton
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Slider
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlin.math.PI
import kotlin.math.sin
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.collect
import uniffi.reprise_android_ffi.AndroidArtworkSize
import uniffi.reprise_android_ffi.AndroidRepeatMode

/**
 * The one honest Android playback surface beyond the mini player.
 *
 * Transport arrives through [LocalPlaybackControls] rather than as parameters:
 * this sheet and the mini player are the only two leaves that issue commands,
 * and everything between them and the activity was forwarding them untouched.
 */
@Composable
internal fun NowPlayingSheet(
    track: LibraryTrack,
    playback: PlaybackUiState,
    close: () -> Unit,
) {
    var backProgress by remember { mutableFloatStateOf(0f) }
    PredictiveBackHandler {
        try {
            it.collect { event -> backProgress = event.progress }
            close()
        } catch (_: CancellationException) {
            backProgress = 0f
        }
    }

    Surface(
        modifier = Modifier
            .fillMaxSize()
            .graphicsLayer {
                translationY = backProgress * 64.dp.toPx()
                scaleX = 1f - backProgress * 0.03f
                scaleY = 1f - backProgress * 0.03f
            },
        color = MaterialTheme.colorScheme.surfaceContainer,
        shape = RoundedCornerShape(
            topStart = nowPlayingMetrics.coverRadiusDp.dp,
            topEnd = nowPlayingMetrics.coverRadiusDp.dp,
        ),
        shadowElevation = 12.dp,
    ) {
        Column(
            modifier = Modifier
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 24.dp, vertical = 12.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Box(
                modifier = Modifier
                    .size(width = 40.dp, height = 4.dp)
                    .clip(MaterialTheme.shapes.extraSmall)
                    .background(MaterialTheme.colorScheme.outline),
            )
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.End,
            ) {
                IconButton(onClick = close, modifier = Modifier.size(48.dp)) {
                    MaterialSymbol("keyboard_arrow_down", "Collapse Now Playing")
                }
            }
            TrackCover(
                trackUri = track.uri,
                size = nowPlayingMetrics.coverSizeDp,
                artworkSize = AndroidArtworkSize.NOW_PLAYING,
                shape = RoundedCornerShape(nowPlayingMetrics.coverRadiusDp.dp),
            )
            Spacer(Modifier.height(20.dp))
            Text(
                text = track.title,
                style = TextStyle(
                    fontSize = nowPlayingMetrics.titleSizeSp.sp,
                    lineHeight = nowPlayingMetrics.titleLineHeightSp.sp,
                    fontWeight = FontWeight.SemiBold,
                ),
                textAlign = TextAlign.Center,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                text = track.artist.ifBlank { "Unknown artist" },
                style = TextStyle(
                    fontSize = nowPlayingMetrics.artistSizeSp.sp,
                    lineHeight = nowPlayingMetrics.artistLineHeightSp.sp,
                ),
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                textAlign = TextAlign.Center,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            WavySeekSlider(trackId = track.id, playback = playback)
            PlaybackActions(playback = playback)
            RatingRow(track = track)
            playback.error?.let { message ->
                Text(
                    text = message,
                    color = MaterialTheme.colorScheme.error,
                    style = MaterialTheme.typography.bodyMedium,
                    textAlign = TextAlign.Center,
                )
            }
            Spacer(Modifier.height(24.dp))
        }
    }
}

@Composable
@OptIn(ExperimentalMaterial3Api::class)
private fun WavySeekSlider(trackId: Long, playback: PlaybackUiState) {
    val seekTo = LocalPlaybackControls.current::seekTo
    var position by remember(trackId) {
        mutableStateOf(SeekPositionState.fromSnapshot(playback.positionMs))
    }
    LaunchedEffect(trackId, playback.positionMs) {
        position = position.acceptSnapshot(playback.positionMs)
    }
    val durationMs = playback.durationMs.coerceAtLeast(0)
    val sliderMaximum = durationMs.coerceAtLeast(1).toFloat()
    val displayed = position.positionMs.coerceIn(0, durationMs.coerceAtLeast(0))
    Column(modifier = Modifier.fillMaxWidth()) {
        Slider(
            value = displayed.toFloat(),
            onValueChange = { value -> position = position.dragTo(value.toLong()) },
            onValueChangeFinished = {
                seekTo(position.positionMs)
                position = position.release()
            },
            valueRange = 0f..sliderMaximum,
            enabled = durationMs > 0,
            track = {
                WavySliderTrack(
                    progress = if (durationMs > 0) {
                        displayed.toFloat() / durationMs.toFloat()
                    } else {
                        0f
                    },
                )
            },
        )
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
        ) {
            Text(
                text = formatDuration(displayed),
                style = MaterialTheme.typography.labelMedium,
                color = MaterialTheme.colorScheme.primary,
            )
            Text(
                text = formatRemaining(displayed, durationMs),
                style = MaterialTheme.typography.labelMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

/**
 * The wave is what has been played. Right of the head the track is still to
 * come, so it stays a flat line — a wave there would claim motion the player
 * has not made yet.
 */
@Composable
private fun WavySliderTrack(progress: Float) {
    val active = MaterialTheme.colorScheme.primary
    val inactive = MaterialTheme.colorScheme.outline
    Canvas(
        modifier = Modifier
            .fillMaxWidth()
            .height(32.dp),
    ) {
        val center = size.height / 2f
        val head = size.width * progress.coerceIn(0f, 1f)
        val thickness = 3.dp.toPx()
        drawLine(
            color = inactive,
            start = Offset(head, center),
            end = Offset(size.width, center),
            strokeWidth = thickness,
            cap = StrokeCap.Round,
        )
        if (head <= 0f) {
            return@Canvas
        }
        val amplitude = 4.dp.toPx()
        val wavelength = 24.dp.toPx()
        val step = 2.dp.toPx()
        fun waveAt(x: Float) = center + sin((x / wavelength) * 2.0 * PI).toFloat() * amplitude
        val elapsed = Path()
        elapsed.moveTo(0f, center)
        var x = 0f
        while (x < head) {
            elapsed.lineTo(x, waveAt(x))
            x += step
        }
        elapsed.lineTo(head, waveAt(head))
        drawPath(elapsed, active, style = Stroke(width = thickness, cap = StrokeCap.Round))
    }
}

@Composable
private fun PlaybackActions(playback: PlaybackUiState) {
    val controls = LocalPlaybackControls.current
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        ModeButton(
            symbol = "shuffle",
            description = if (playback.shuffled) "Turn shuffle off" else "Turn shuffle on",
            active = playback.shuffled,
            onClick = { controls.setShuffle(!playback.shuffled) },
        )
        IconButton(onClick = controls::previous, modifier = Modifier.size(48.dp)) {
            MaterialSymbol("skip_previous", "Previous track", sizeSp = 30)
        }
        IconButton(
            onClick = controls::togglePause,
            modifier = Modifier
                .size(nowPlayingMetrics.playButtonSizeDp.dp)
                // The shape scale's top rung is the frame's 28 dp rounded
                // square; a circle would be a different control.
                .clip(MaterialTheme.shapes.extraLarge)
                .background(MaterialTheme.colorScheme.primary),
        ) {
            MaterialSymbol(
                name = if (playback.isPlaying) "pause" else "play_arrow",
                contentDescription = playback.playPauseLabel,
                tint = MaterialTheme.colorScheme.onPrimary,
                sizeSp = 40,
            )
        }
        IconButton(onClick = controls::next, modifier = Modifier.size(48.dp)) {
            MaterialSymbol("skip_next", "Next track", sizeSp = 30)
        }
        ModeButton(
            symbol = if (playback.repeat == AndroidRepeatMode.ONE) "repeat_one" else "repeat",
            description = "Repeat ${playback.repeat.name.lowercase()}",
            active = playback.repeat != AndroidRepeatMode.OFF,
            onClick = { controls.setRepeat(cycleRepeatMode(playback.repeat)) },
        )
    }
}

@Composable
private fun ModeButton(
    symbol: String,
    description: String,
    active: Boolean,
    onClick: () -> Unit,
) {
    IconButton(
        onClick = onClick,
        modifier = Modifier
            .size(48.dp)
            .then(
                if (active) {
                    Modifier
                        .clip(MaterialTheme.shapes.large)
                        .background(MaterialTheme.colorScheme.secondaryContainer)
                } else {
                    Modifier
                },
            ),
    ) {
        MaterialSymbol(
            name = symbol,
            contentDescription = description,
            tint = if (active) {
                MaterialTheme.colorScheme.onSecondaryContainer
            } else {
                MaterialTheme.colorScheme.onSurfaceVariant
            },
        )
    }
}

/**
 * `setRating` answers with the failure to show, or null when the rating was
 * saved. That message is an acknowledgement rather than state — the star does
 * not move, so without it the tap looks like it worked — which is why it gets a
 * [TransientMessage] and the two errors on the browse screen do not. See that
 * type for the rule.
 */
@Composable
private fun RatingRow(track: LibraryTrack) {
    val setRating = LocalPlaybackControls.current::setRating
    var rating by remember(track.id) { mutableStateOf(track.rating.coerceIn(0, 5)) }
    var failure by remember(track.id) { mutableStateOf<TransientMessage?>(null) }
    Column(horizontalAlignment = Alignment.CenterHorizontally) {
        Text(
            text = "${track.playCount.coerceAtLeast(0)} plays",
            style = MaterialTheme.typography.labelLarge,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Row {
            (1..5).forEach { star ->
                IconButton(
                    onClick = {
                        val message = setRating(track.id, star)
                        if (message == null) {
                            rating = star
                            failure = null
                        } else {
                            failure = TransientMessage(message).after(failure)
                        }
                    },
                    modifier = Modifier
                        .size(48.dp)
                        .semantics { selected = star <= rating },
                ) {
                    MaterialSymbol(
                        name = if (star <= rating) "star" else "star_outline",
                        contentDescription = "Rate $star of 5 stars",
                        tint = MaterialTheme.colorScheme.tertiary,
                        sizeSp = 28,
                    )
                }
            }
        }
        TransientMessageText(failure) { failure = null }
    }
}
