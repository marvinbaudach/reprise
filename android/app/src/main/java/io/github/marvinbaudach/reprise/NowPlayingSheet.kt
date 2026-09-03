package io.github.marvinbaudach.reprise

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
import androidx.compose.animation.core.CubicBezierEasing
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
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.RectangleShape
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.testTagsAsResourceId
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.viewmodel.compose.viewModel
import io.github.marvinbaudach.reprise.ui.theme.AmbientTrueBlack
import io.github.marvinbaudach.reprise.ui.theme.NowPlayingOnBackdrop
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
    trackIsStale: Boolean = false,
    close: () -> Unit,
) {
    val metrics = nowPlayingMetrics(surfaceLayout)
    val controls = LocalPlaybackControls.current
    val motion = LocalAmbientMotionController.current
    val visualizerPreference = LocalVisualizerPreference.current
    val currentIndex = playback.currentIndex ?: 0
    val panelWindow = rememberPlayPanelWindow(track, currentIndex, controls, trackIsStale)
    val positionPx = remember { Animatable(0f) }
    val verticalOffset = remember { Animatable(0f) }
    val gestureScope = rememberCoroutineScope()
    var screenWidthPx by remember { mutableFloatStateOf(0f) }
    var draggingTrack by remember { mutableStateOf(false) }
    var settlingTargetIndex by remember { mutableStateOf<Int?>(null) }
    val positionReconciler = remember { NowPlayingPositionReconciler() }
    val cueGate = remember { TrackChangeCueGate() }
    var cueRevision by remember { mutableIntStateOf(0) }
    val haptics = rememberQueueHaptics()
    val latestCurrentIndex by rememberUpdatedState(currentIndex)
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
    val visualizerIntent = remember(visualizerPreference) { PendingToggleIntent() }
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
    LaunchedEffect(track.id, currentIndex, screenWidthPx, motion.sceneAnimationsEnabled) {
        if (screenWidthPx <= 0f) return@LaunchedEffect
        val target = currentIndex * screenWidthPx
        when (
            positionReconciler.update(
                trackId = track.id,
                index = currentIndex,
                dragging = draggingTrack,
                animationsEnabled = motion.sceneAnimationsEnabled,
                settlingTargetIndex = settlingTargetIndex,
            )
        ) {
            NowPlayingPositionAction.ANIMATE -> positionPx.animateTo(
                target,
                tween(NOW_PLAYING_SETTLE_MS, easing = NOW_PLAYING_SETTLE_EASING),
            )
            NowPlayingPositionAction.SNAP,
            NowPlayingPositionAction.REANCHOR,
            -> positionPx.snapTo(target)
            NowPlayingPositionAction.CONTINUE_SETTLE -> Unit
        }
    }
    LaunchedEffect(track.id, motion.sceneAnimationsEnabled) {
        if (cueGate.observe(track.id, motion.sceneAnimationsEnabled)) {
            cueRevision += 1
            haptics.commit()
        }
    }
    val settleTrack: (PlayGestureDecision) -> Unit = { decision ->
        val requestedIndex = when (decision) {
            PlayGestureDecision.NEXT -> currentIndex + 1
            PlayGestureDecision.PREVIOUS -> currentIndex - 1
            else -> currentIndex
        }
        val targetIndex = requestedIndex.coerceIn(panelWindow.firstIndex, panelWindow.lastIndex)
        val changesTrack = targetIndex != currentIndex
        gestureScope.launch {
            val target = targetIndex * screenWidthPx
            if (changesTrack && motion.sceneAnimationsEnabled) {
                settlingTargetIndex = targetIndex
            }
            try {
                when (decision) {
                    PlayGestureDecision.NEXT -> if (changesTrack) controls.next()
                    PlayGestureDecision.PREVIOUS -> if (changesTrack) {
                        controls.previousInQueueOrder()
                    }
                    else -> Unit
                }
                settleNowPlayingPosition(
                    target = target,
                    animationsEnabled = motion.sceneAnimationsEnabled,
                    animate = { targetValue ->
                        positionPx.animateTo(
                            targetValue = targetValue,
                            animationSpec = tween(
                                NOW_PLAYING_SETTLE_MS,
                                easing = NOW_PLAYING_SETTLE_EASING,
                            ),
                        )
                    },
                    snap = positionPx::snapTo,
                )
                if (
                    changesTrack &&
                    latestCurrentIndex == currentIndex
                ) {
                    positionPx.snapTo(currentIndex * screenWidthPx)
                }
            } finally {
                if (settlingTargetIndex == targetIndex) settlingTargetIndex = null
            }
        }
    }

    Surface(
        modifier = Modifier
            .fillMaxSize()
            .onSizeChanged { size ->
                val width = size.width.toFloat()
                if (screenWidthPx == width) return@onSizeChanged
                screenWidthPx = width
                gestureScope.launch { positionPx.snapTo(currentIndex * width) }
            }
            .testTag("now-playing-gestures")
            .nowPlayingGestures(
                animationsEnabled = motion.sceneAnimationsEnabled,
                currentIndex = currentIndex,
                firstIndex = panelWindow.firstIndex,
                lastIndex = panelWindow.lastIndex,
                positionPx = positionPx.value,
                onHorizontalPosition = { position ->
                    gestureScope.launch { positionPx.snapTo(position) }
                },
                onVerticalOffset = { offset ->
                    gestureScope.launch { verticalOffset.snapTo(offset) }
                },
                onDragStateChanged = { draggingTrack = it },
                onSettle = { decision ->
                    when (decision) {
                        PlayGestureDecision.NEXT,
                        PlayGestureDecision.PREVIOUS,
                        PlayGestureDecision.SPRING_BACK,
                        -> settleTrack(decision)
                        PlayGestureDecision.DISMISS -> close()
                    }
                    gestureScope.launch {
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
                    val showSpectrum = visualizerIntent.next(visualizerVisible.value)
                    visualizerPreference.setVisualizer(
                        choice = if (showSpectrum) {
                            AndroidVisualizerChoice.SPECTRUM
                        } else {
                            AndroidVisualizerChoice.COVER
                        },
                        report = { outcome ->
                            outcome.onSuccess { visualizerVisible.value = showSpectrum }
                                .onFailure { error ->
                                    Log.e(
                                        NOW_PLAYING_VISUALIZER_TAG,
                                        "Could not save the visualizer choice",
                                        error,
                                    )
                                }
                            visualizerIntent.answered(showSpectrum)
                        },
                    )
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
                onPrevious = { settleTrack(PlayGestureDecision.PREVIOUS) },
                onNext = { settleTrack(PlayGestureDecision.NEXT) },
            )
        } else {
            Box(Modifier.fillMaxSize().testTag("now-playing-content")) {
                NowPlayingScene(
                    track = track,
                    playback = playback,
                    surfaceState = surfaceState,
                    positionPx = positionPx.value,
                    currentIndex = currentIndex,
                    panels = panelWindow.panels,
                    visualizerOpacity = visualizerOpacity.value,
                    cueRevision = cueRevision,
                    onCoverBounds = { coverBounds.value = it },
                    onPrevious = { settleTrack(PlayGestureDecision.PREVIOUS) },
                    onNext = { settleTrack(PlayGestureDecision.NEXT) },
                )
                TopEdgeAccentLine(
                    deviationPx = positionPx.value - currentIndex * screenWidthPx,
                    widthPx = screenWidthPx,
                    fingerDown = draggingTrack,
                    animationsEnabled = motion.sceneAnimationsEnabled,
                )
                TopEdgeSweep(cueRevision, motion.sceneAnimationsEnabled)
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
                playback.faultNotice?.let { message ->
                    Text(
                        text = message.text,
                        modifier = Modifier
                            .align(Alignment.TopCenter)
                            .background(
                                AmbientTrueBlack.copy(alpha = 0.72f),
                                RoundedCornerShape(18.dp),
                            )
                            .padding(horizontal = 18.dp, vertical = 10.dp)
                            .testTag("playback-fault-notice"),
                        color = MaterialTheme.colorScheme.error,
                        style = MaterialTheme.typography.bodyMedium,
                        textAlign = TextAlign.Center,
                    )
                }
            }
        }
    }
}

internal suspend fun settleNowPlayingPosition(
    target: Float,
    animationsEnabled: Boolean,
    animate: suspend (Float) -> Unit,
    snap: suspend (Float) -> Unit,
) {
    if (animationsEnabled) animate(target) else snap(target)
}

internal const val VISUALIZER_CROSSFADE_MS = 220
internal const val NOW_PLAYING_SETTLE_MS = 480
internal val NOW_PLAYING_SETTLE_EASING = CubicBezierEasing(0.22f, 1.06f, 0.32f, 1f)
private const val NOW_PLAYING_VISUALIZER_TAG = "RepriseVisualizer"

@Composable
private fun WideShortNowPlayingContent(
    track: LibraryTrack,
    playback: PlaybackUiState,
    surfaceState: MobileSurfaceViewModel,
    metrics: NowPlayingMetrics,
    onPrevious: () -> Unit,
    onNext: () -> Unit,
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
            playback.faultNotice?.let { message ->
                Text(
                    text = message.text,
                    color = MaterialTheme.colorScheme.error,
                    style = MaterialTheme.typography.bodyMedium,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            Spacer(Modifier.weight(1f))
            PlaybackActions(
                playback = playback,
                metrics = metrics,
                wideShort = true,
                onPrevious = onPrevious,
                onNext = onNext,
            )
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
    cueRevision: Int = 0,
    animationsEnabled: Boolean = true,
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
    Column(
        modifier = Modifier.fillMaxWidth().semantics { testTagsAsResourceId = true },
    ) {
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
            thumb = {},
            track = {
                SpectralSeekTrack(
                    trackId,
                    displayed,
                    durationMs,
                    cueRevision,
                    animationsEnabled,
                )
            },
        )
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
        ) {
            Text(
                text = formatDuration(displayed),
                modifier = Modifier.testTag("now-playing-position"),
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
    onPrevious: () -> Unit,
    onNext: () -> Unit,
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
        IconButton(onClick = onPrevious, modifier = Modifier.size(48.dp)) {
            MaterialSymbol("skip_previous", "Previous track", sizeSp = 30)
        }
        if (wideShort) {
            IconButton(onClick = onNext, modifier = Modifier.size(48.dp)) {
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
            IconButton(onClick = onNext, modifier = Modifier.size(48.dp)) {
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
