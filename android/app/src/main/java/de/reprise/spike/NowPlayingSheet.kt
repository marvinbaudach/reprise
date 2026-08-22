package de.reprise.spike

import android.util.Log
import androidx.activity.compose.PredictiveBackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.interaction.DragInteraction
import androidx.compose.foundation.interaction.MutableInteractionSource
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
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Slider
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.RectangleShape
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.viewmodel.compose.viewModel
import de.reprise.spike.ui.theme.AmbientTrueBlack
import de.reprise.spike.ui.theme.NowPlayingOnBackdrop
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.launch
import uniffi.reprise_android_ffi.AndroidArtworkSize
import uniffi.reprise_android_ffi.AndroidRepeatMode
import uniffi.reprise_android_ffi.AndroidStoredVisualizer
import uniffi.reprise_android_ffi.AndroidVisualizerChoice

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
    val controls = LocalPlaybackControls.current
    val motion = LocalAmbientMotionController.current
    val visualizerPreference = LocalVisualizerPreference.current
    val neighbours = rememberPlayGestureNeighbours(track, controls)
    val horizontalOffset = remember { Animatable(0f) }
    val verticalOffset = remember { Animatable(0f) }
    val gestureScope = rememberCoroutineScope()
    var seekMarker by remember { mutableStateOf<String?>(null) }
    var seekMarkerRevision by remember { mutableIntStateOf(0) }
    var backProgress by remember { mutableFloatStateOf(0f) }
    val coverBounds = remember { mutableStateOf(Rect.Zero) }
    val visualizerVisible = remember(visualizerPreference) {
        mutableStateOf(
            runCatching(visualizerPreference::visualizerSetting)
                .onFailure { error ->
                    Log.e(NOW_PLAYING_VISUALIZER_TAG, "Could not read the visualizer choice", error)
                }
                .getOrDefault(AndroidStoredVisualizer.Unset)
                .showsSpectrum(),
        )
    }
    val visualizerOpacity = remember(visualizerPreference) {
        Animatable(if (visualizerVisible.value) 1f else 0f)
    }
    LaunchedEffect(visualizerVisible.value, motion.sceneAnimationsEnabled) {
        val target = if (visualizerVisible.value) 1f else 0f
        if (motion.sceneAnimationsEnabled) {
            visualizerOpacity.animateTo(target, tween(VISUALIZER_CROSSFADE_MS))
        } else {
            visualizerOpacity.snapTo(target)
        }
    }
    LaunchedEffect(seekMarkerRevision) {
        if (seekMarkerRevision == 0) return@LaunchedEffect
        delay(600)
        seekMarker = null
    }
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
            .testTag("now-playing-gestures")
            .nowPlayingGestures(
                animationsEnabled = motion.sceneAnimationsEnabled,
                onHorizontalOffset = { offset ->
                    gestureScope.launch { horizontalOffset.snapTo(offset) }
                },
                onVerticalOffset = { offset ->
                    gestureScope.launch { verticalOffset.snapTo(offset) }
                },
                onSettle = { decision ->
                    when (decision) {
                        PlayGestureDecision.NEXT -> controls.next()
                        PlayGestureDecision.PREVIOUS -> controls.previous()
                        PlayGestureDecision.DISMISS -> close()
                        PlayGestureDecision.SPRING_BACK -> Unit
                    }
                    gestureScope.launch {
                        horizontalOffset.animateTo(0f, spring())
                        verticalOffset.animateTo(0f, spring())
                    }
                },
                onDoubleTap = { leftHalf ->
                    val delta = if (leftHalf) -10_000L else 10_000L
                    controls.seekTo((playback.positionMs + delta).coerceIn(0L, playback.durationMs))
                    seekMarker = if (leftHalf) "−10 s" else "+10 s"
                    seekMarkerRevision += 1
                },
                onTap = { position ->
                    if (!coverBounds.value.contains(position)) return@nowPlayingGestures
                    val showSpectrum = !visualizerVisible.value
                    runCatching {
                        visualizerPreference.setVisualizer(
                            if (showSpectrum) {
                                AndroidVisualizerChoice.SPECTRUM
                            } else {
                                AndroidVisualizerChoice.COVER
                            },
                        )
                    }.onSuccess {
                        visualizerVisible.value = showSpectrum
                    }.onFailure { error ->
                        Log.e(NOW_PLAYING_VISUALIZER_TAG, "Could not save the visualizer choice", error)
                    }
                },
            )
            .graphicsLayer {
                translationY = backProgress * 64.dp.toPx() + verticalOffset.value
                scaleX = 1f - backProgress * 0.03f
                scaleY = 1f - backProgress * 0.03f
            },
        color = if (surfaceLayout == SurfaceLayout.WIDE_SHORT) {
            MaterialTheme.colorScheme.surfaceContainer
        } else {
            AmbientTrueBlack
        },
        shape = if (surfaceLayout == SurfaceLayout.WIDE_SHORT) {
            RoundedCornerShape(
                topStart = nowPlayingMetrics.coverRadiusDp.dp,
                topEnd = nowPlayingMetrics.coverRadiusDp.dp,
            )
        } else {
            RectangleShape
        },
        shadowElevation = 12.dp,
    ) {
        if (surfaceLayout == SurfaceLayout.WIDE_SHORT) {
            WideShortNowPlayingContent(
                track = track,
                playback = playback,
                surfaceState = surfaceState,
                metrics = metrics,
            )
        } else {
            Box(Modifier.fillMaxSize().testTag("now-playing-content")) {
                NowPlayingScene(
                    track = track,
                    playback = playback,
                    surfaceState = surfaceState,
                    horizontalOffsetPx = horizontalOffset.value,
                    previousTrack = neighbours.previous,
                    nextTrack = neighbours.next,
                    visualizerOpacity = visualizerOpacity.value,
                    onCoverBounds = { coverBounds.value = it },
                )
                seekMarker?.let { marker ->
                    Text(
                        text = marker,
                        modifier = Modifier
                            .align(Alignment.Center)
                            .background(
                                AmbientTrueBlack.copy(alpha = 0.72f),
                                RoundedCornerShape(18.dp),
                            )
                            .padding(horizontal = 18.dp, vertical = 10.dp)
                            .testTag("now-playing-seek-marker"),
                        color = NowPlayingOnBackdrop,
                        style = MaterialTheme.typography.titleMedium,
                    )
                }
            }
        }
    }
}

internal const val VISUALIZER_CROSSFADE_MS = 220
private const val NOW_PLAYING_VISUALIZER_TAG = "RepriseVisualizer"

@Composable
private fun WideShortNowPlayingContent(
    track: LibraryTrack,
    playback: PlaybackUiState,
    surfaceState: MobileSurfaceViewModel,
    metrics: NowPlayingMetrics,
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
            TrackCover(
                trackUri = track.uri,
                title = track.title,
                artist = track.artist,
                size = metrics.coverSizeDp,
                modifier = Modifier.testTag("now-playing-cover"),
                artworkSize = AndroidArtworkSize.NOW_PLAYING,
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
                modifier = Modifier
                    .fillMaxWidth()
                    .testTag("now-playing-actions"),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Column(modifier = Modifier.weight(1f)) {
                    Text(
                        text = track.title,
                        modifier = Modifier.testTag("now-playing-title"),
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
                SleepTimerControl(playback.sleepTimer)
                FavouriteHeartButton(
                    track = track,
                    surfaceState = surfaceState,
                    tag = "now-playing-heart",
                    enabled = LocalNowPlayingActionsEnabled.current,
                )
                // No collapse button beside it: this sheet is dismissed by
                // swiping it down, which is what freed the slot the context
                // menu now uses.
                NowPlayingTrackContextMenu(track)
            }
            SpectralSeekSlider(trackId = track.id, playback = playback, surfaceState = surfaceState)
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
internal fun SpectralSeekSlider(
    trackId: Long,
    playback: PlaybackUiState,
    surfaceState: MobileSurfaceViewModel,
    interactionSource: MutableInteractionSource? = null,
) {
    val seekTo = LocalPlaybackControls.current::seekTo
    val sliderInteractionSource = interactionSource ?: remember { MutableInteractionSource() }
    LaunchedEffect(sliderInteractionSource, trackId) {
        sliderInteractionSource.interactions.collect { interaction ->
            if (interaction is DragInteraction.Cancel) {
                surfaceState.releaseScrub(trackId)
            }
        }
    }
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
                surfaceState.releaseScrub(trackId)?.let { released ->
                    seekTo(released.positionMs)
                }
            },
            interactionSource = sliderInteractionSource,
            valueRange = 0f..sliderMaximum,
            enabled = durationMs > 0,
            track = { SpectralSeekTrack(trackId, displayed, durationMs) },
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
                name = playback.playPauseSymbol,
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
