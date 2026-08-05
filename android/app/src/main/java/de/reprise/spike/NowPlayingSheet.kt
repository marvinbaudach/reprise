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
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
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
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.viewmodel.compose.viewModel
import kotlin.math.PI
import kotlin.math.sin
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.collect
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
    surfaceLayout: SurfaceLayout = SurfaceLayout.STACKED,
    surfaceState: MobileSurfaceViewModel = viewModel(),
    close: () -> Unit,
) {
    val metrics = nowPlayingMetrics(surfaceLayout)
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
        if (surfaceLayout == SurfaceLayout.WIDE_SHORT) {
            WideShortNowPlayingContent(
                track = track,
                playback = playback,
                surfaceState = surfaceState,
                metrics = metrics,
                close = close,
            )
        } else {
            StackedNowPlayingContent(
                track = track,
                playback = playback,
                surfaceState = surfaceState,
                metrics = metrics,
                close = close,
            )
        }
    }
}

@Composable
private fun StackedNowPlayingContent(
    track: LibraryTrack,
    playback: PlaybackUiState,
    surfaceState: MobileSurfaceViewModel,
    metrics: NowPlayingMetrics,
    close: () -> Unit,
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
            NowPlayingVisualizer(
                trackUri = track.uri,
                size = metrics.coverSizeDp,
                shape = RoundedCornerShape(metrics.coverRadiusDp.dp),
            )
            Spacer(Modifier.height(20.dp))
            Text(
                text = track.title,
                style = TextStyle(
                    fontSize = metrics.titleSizeSp.sp,
                    lineHeight = metrics.titleLineHeightSp.sp,
                    fontWeight = FontWeight.SemiBold,
                ),
                textAlign = TextAlign.Center,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                text = track.artist.ifBlank { "Unknown artist" },
                style = TextStyle(
                    fontSize = metrics.artistSizeSp.sp,
                    lineHeight = metrics.artistLineHeightSp.sp,
                ),
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                textAlign = TextAlign.Center,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            WavySeekSlider(trackId = track.id, playback = playback, surfaceState = surfaceState)
            PlaybackActions(playback = playback, metrics = metrics, wideShort = false)
            RatingRow(track = track, surfaceState = surfaceState)
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

@Composable
private fun WideShortNowPlayingContent(
    track: LibraryTrack,
    playback: PlaybackUiState,
    surfaceState: MobileSurfaceViewModel,
    metrics: NowPlayingMetrics,
    close: () -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = 16.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(
            modifier = Modifier
                .width(metrics.coverSizeDp.dp)
                .fillMaxHeight(),
            contentAlignment = Alignment.Center,
        ) {
            NowPlayingVisualizer(
                trackUri = track.uri,
                size = metrics.coverSizeDp,
                shape = RoundedCornerShape(metrics.coverRadiusDp.dp),
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
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Column(modifier = Modifier.weight(1f)) {
                    Text(
                        text = track.title,
                        style = TextStyle(
                            fontSize = metrics.titleSizeSp.sp,
                            lineHeight = metrics.titleLineHeightSp.sp,
                            fontWeight = FontWeight.SemiBold,
                        ),
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                    Text(
                        text = track.artist.ifBlank { "Unknown artist" },
                        style = TextStyle(
                            fontSize = metrics.artistSizeSp.sp,
                            lineHeight = metrics.artistLineHeightSp.sp,
                        ),
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
                IconButton(onClick = close, modifier = Modifier.size(48.dp)) {
                    MaterialSymbol("keyboard_arrow_down", "Collapse Now Playing")
                }
            }
            RatingRow(track = track, surfaceState = surfaceState, wideShort = true)
            WavySeekSlider(trackId = track.id, playback = playback, surfaceState = surfaceState)
            playback.error?.let { message ->
                Text(
                    text = message,
                    color = MaterialTheme.colorScheme.error,
                    style = MaterialTheme.typography.bodyMedium,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            Spacer(Modifier.weight(1f))
            PlaybackActions(playback = playback, metrics = metrics, wideShort = true)
        }
    }
}

@Composable
@OptIn(ExperimentalMaterial3Api::class)
private fun WavySeekSlider(
    trackId: Long,
    playback: PlaybackUiState,
    surfaceState: MobileSurfaceViewModel,
) {
    val seekTo = LocalPlaybackControls.current::seekTo
    val position = surfaceState.seekPosition(trackId, playback.positionMs)
    LaunchedEffect(trackId, playback.positionMs) {
        surfaceState.acceptPlaybackSnapshot(trackId, playback.positionMs)
    }
    val durationMs = playback.durationMs.coerceAtLeast(0)
    val sliderMaximum = durationMs.coerceAtLeast(1).toFloat()
    val displayed = position.positionMs.coerceIn(0, durationMs.coerceAtLeast(0))
    Column(modifier = Modifier.fillMaxWidth()) {
        Slider(
            modifier = Modifier.testTag("now-playing-seek"),
            value = displayed.toFloat(),
            onValueChange = { value -> surfaceState.dragTo(trackId, value.toLong()) },
            onValueChangeFinished = {
                seekTo(surfaceState.releaseScrub(trackId).positionMs)
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
private fun PlaybackActions(
    playback: PlaybackUiState,
    metrics: NowPlayingMetrics,
    wideShort: Boolean,
) {
    val controls = LocalPlaybackControls.current
    Row(
        modifier = Modifier.fillMaxWidth().testTag("now-playing-transport"),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (!wideShort) {
            ModeButton(
                symbol = "shuffle",
                description = if (playback.shuffled) "Turn shuffle off" else "Turn shuffle on",
                active = playback.shuffled,
                onClick = { controls.setShuffle(!playback.shuffled) },
            )
        }
        IconButton(onClick = controls::previous, modifier = Modifier.size(48.dp)) {
            MaterialSymbol("skip_previous", "Previous track", sizeSp = 30)
        }
        if (wideShort) {
            IconButton(onClick = controls::next, modifier = Modifier.size(48.dp)) {
                MaterialSymbol("skip_next", "Next track", sizeSp = 30)
            }
        }
        IconButton(
            onClick = controls::togglePause,
            modifier = Modifier
                .size(metrics.playButtonSizeDp.dp)
                .testTag("now-playing-play")
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
        if (!wideShort) {
            IconButton(onClick = controls::next, modifier = Modifier.size(48.dp)) {
                MaterialSymbol("skip_next", "Next track", sizeSp = 30)
            }
        } else {
            ModeButton(
                symbol = "shuffle",
                description = if (playback.shuffled) "Turn shuffle off" else "Turn shuffle on",
                active = playback.shuffled,
                onClick = { controls.setShuffle(!playback.shuffled) },
            )
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
 * `setRating` writes off the main thread and answers back on it, with the
 * failure to show or null when the rating was saved. The star waits for that
 * answer: it is never moved in advance and never moved back, because a star
 * that moved before the database agreed would be telling the user something
 * nobody has checked.
 *
 * The failure message is an acknowledgement rather than state — the star does
 * not move, so without it the tap looks like it worked — which is why it gets a
 * [TransientMessage] and the two errors on the browse screen do not. See that
 * type for the rule.
 *
 * The rating itself is not kept here. It is one value with three surfaces
 * showing it, and a `remember`ed copy per surface is how the dock's star and
 * these five came to disagree — see [MobileSurfaceViewModel.ratingOf]. The
 * failure is genuinely this row's own, and stays keyed on the track so an
 * answer arriving after the sheet has moved on lands in state nobody is showing
 * rather than under the next track's stars.
 */
@Composable
private fun RatingRow(
    track: LibraryTrack,
    surfaceState: MobileSurfaceViewModel,
    wideShort: Boolean = false,
) {
    val setRating = LocalPlaybackControls.current::setRating
    val rating = surfaceState.ratingOf(track)
    var failure by remember(track.id) { mutableStateOf<TransientMessage?>(null) }
    val content: @Composable () -> Unit = {
        Text(
            text = "${track.playCount.coerceAtLeast(0)} plays",
            style = MaterialTheme.typography.labelLarge,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Row {
            (1..5).forEach { star ->
                IconButton(
                    onClick = {
                        setRating(track.id, star) { message ->
                            if (message == null) {
                                surfaceState.confirmRating(track.id, rating, star)
                                failure = null
                            } else {
                                failure = TransientMessage(message).after(failure)
                            }
                        }
                    },
                    // `selected` would claim these five are one exclusive choice
                    // inside a selectable group, which they are not: up to five
                    // of them are filled at once and there is no group. The
                    // rating is a *state*, so it is carried as one — and as the
                    // whole control's state, so a screen reader user learns the
                    // current rating from whichever star they land on rather
                    // than by counting filled ones.
                    modifier = Modifier
                        .size(48.dp)
                        .semantics { stateDescription = "Rated $rating of 5" },
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
    if (wideShort) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            content()
        }
    } else {
        Column(horizontalAlignment = Alignment.CenterHorizontally) {
            content()
        }
    }
}
