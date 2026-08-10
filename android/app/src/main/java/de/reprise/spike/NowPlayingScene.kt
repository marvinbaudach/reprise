package de.reprise.spike

import android.content.SharedPreferences
import android.util.Log
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Slider
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.RoundRect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.clipPath
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import de.reprise.spike.scene.CoreShape
import de.reprise.spike.scene.SceneState
import de.reprise.spike.scene.SpectrogramFrames
import kotlinx.coroutines.delay
import uniffi.reprise_android_ffi.AndroidArtworkSize
import uniffi.reprise_android_ffi.AndroidNowPlayingViewChoice
import uniffi.reprise_android_ffi.AndroidRepeatMode
import uniffi.reprise_android_ffi.AndroidStoredNowPlayingView
import uniffi.reprise_android_ffi.MusicLibrary
import kotlin.math.roundToInt

private const val TRANSITION_MS = 320
private const val CONTROL_IDLE_MS = 4_000L
private const val CONTROL_FADE_MS = 300

/** Below this opacity a control is off the screen, so it must be off the touch map too. */
private const val CONTROL_HIT_EPSILON = 0.01f

private const val COVER_SIZE_DP = 272
private const val COVER_RADIUS_DP = 18f
private const val PLAYED_CENTRE_FRACTION = 0.34f
private const val NOW_PLAYING_TAG = "RepriseNowPlaying"

internal enum class NowPlayingView {
    PLAYER,
    VISUALIZER,
}

internal interface NowPlayingViewSettings {
    fun current(): NowPlayingView
    fun set(view: NowPlayingView)
}

internal class AndroidNowPlayingViewSettings(
    private val library: MusicLibrary,
) : NowPlayingViewSettings {
    override fun current(): NowPlayingView = when (library.nowPlayingViewSetting()) {
        AndroidStoredNowPlayingView.Visualizer -> NowPlayingView.VISUALIZER
        AndroidStoredNowPlayingView.Player,
        AndroidStoredNowPlayingView.Unset,
        is AndroidStoredNowPlayingView.Unsupported,
        -> NowPlayingView.PLAYER
    }

    override fun set(view: NowPlayingView) {
        library.setNowPlayingView(
            when (view) {
                NowPlayingView.PLAYER -> AndroidNowPlayingViewChoice.PLAYER
                NowPlayingView.VISUALIZER -> AndroidNowPlayingViewChoice.VISUALIZER
            },
        )
    }
}

/** Persisted substitute used only by JVM-injected activity surfaces with no native library. */
internal class InjectedNowPlayingViewSettings(
    private val preferences: SharedPreferences,
) : NowPlayingViewSettings {
    override fun current(): NowPlayingView = when (
        preferences.getString(INJECTED_NOW_PLAYING_VIEW_KEY, null)
    ) {
        "visualizer" -> NowPlayingView.VISUALIZER
        else -> NowPlayingView.PLAYER
    }

    override fun set(view: NowPlayingView) {
        preferences.edit()
            .putString(
                INJECTED_NOW_PLAYING_VIEW_KEY,
                if (view == NowPlayingView.VISUALIZER) "visualizer" else "player",
            )
            .apply()
    }
}

internal const val INJECTED_NOW_PLAYING_VIEW_KEY = "injected_now_playing_view"

internal val LocalNowPlayingViewSettings = staticCompositionLocalOf<NowPlayingViewSettings> {
    object : NowPlayingViewSettings {
        override fun current() = NowPlayingView.PLAYER
        override fun set(view: NowPlayingView) = Unit
    }
}

@Composable
internal fun NowPlayingScene(
    track: LibraryTrack,
    playback: PlaybackUiState,
    surfaceState: MobileSurfaceViewModel,
    close: () -> Unit,
) {
    val settings = LocalNowPlayingViewSettings.current
    var view by remember(settings) {
        mutableStateOf(
            runCatching(settings::current)
                .onFailure { error ->
                    Log.e(NOW_PLAYING_TAG, "Could not restore Now Playing view", error)
                }
                .getOrDefault(NowPlayingView.PLAYER),
        )
    }
    val visualizer = view == NowPlayingView.VISUALIZER
    val transition by animateFloatAsState(
        targetValue = if (visualizer) 1f else 0f,
        animationSpec = tween(TRANSITION_MS, easing = FastOutSlowInEasing),
        label = "now-playing-scene-transition",
    )
    val frames = rememberSpectrogram(track.id)
    val state = remember(frames, track.title, track.artist) {
        SceneState(frames, CoreShape(track.title, track.artist))
    }
    val artwork = rememberTrackArtworkVisual(track.uri, AndroidArtworkSize.NOW_PLAYING)
    val fallback = MaterialTheme.colorScheme.primary
    val fog = rememberCoverFogBitmap(artwork?.image, fallback)
    val motion = LocalAmbientMotionController.current
    val drawRevision = if (frames.frameCount == 0) {
        0
    } else {
        DriveScene(
            frames = frames,
            state = state,
            playback = playback,
            controller = motion,
            transitionRunning = transition > 0f && transition < 1f,
        )
    }
    val power = motion.sceneRenderPower()
    val bloom = rememberBurstBloomBuffer()
    var controlsVisible by remember { mutableStateOf(true) }
    var touchRevision by remember { mutableIntStateOf(0) }
    LaunchedEffect(visualizer, touchRevision) {
        controlsVisible = true
        if (visualizer) {
            delay(CONTROL_IDLE_MS)
            controlsVisible = false
        }
    }
    val controlAlpha by animateFloatAsState(
        targetValue = if (!visualizer || controlsVisible) 1f else 0f,
        animationSpec = tween(CONTROL_FADE_MS),
        label = "now-playing-control-fade",
    )

    fun wakeControls() {
        controlsVisible = true
        touchRevision += 1
    }

    fun choose(next: NowPlayingView) {
        view = next
        wakeControls()
        runCatching { settings.set(next) }
            .onFailure { error -> Log.e(NOW_PLAYING_TAG, "Could not remember Now Playing view", error) }
    }

    BoxWithConstraints(
        modifier = Modifier
            .fillMaxSize()
            .background(Color.Black)
            .testTag(if (visualizer) "now-playing-visualizer" else "now-playing-player"),
    ) {
        val coverTop = maxHeight * PLAYED_CENTRE_FRACTION - (COVER_SIZE_DP / 2).dp
        Canvas(
            modifier = Modifier
                .fillMaxSize()
                .testTag("now-playing-scene")
                .clickable(
                    interactionSource = remember { MutableInteractionSource() },
                    indication = null,
                ) { wakeControls() },
        ) {
            // Capturing the frame counter is what makes Compose re-run this lambda once
            // per scene frame; the value itself has nothing to contribute to the drawing.
            observeSceneFrame(drawRevision)
            drawRect(Color.Black)
            val playedCenter = Offset(size.width / 2f, size.height * PLAYED_CENTRE_FRACTION)
            drawNowPlayingFog(
                fog = fog,
                center = playedCenter,
                angleA = state.fogAngleA,
                angleB = state.fogAngleB,
                fogLevel = state.fogLevel,
                opacity = 1f - transition,
                rotationsEnabled = power.fogRotates,
            )
            drawPlayedCover(
                artwork = artwork?.image,
                center = playedCenter,
                fallback = fallback,
                opacity = 1f - transition,
                scale = 1f - transition * 0.14f,
            )
            drawNowPlayingBurst(
                state = state,
                bloomBuffer = bloom,
                opacity = transition,
                effects = power.burstEffects,
            )
        }

        Box(
            modifier = Modifier
                .align(Alignment.TopCenter)
                .offset(y = coverTop - ((nowPlayingMetrics.coverSizeDp - COVER_SIZE_DP) / 2).dp)
                .size(nowPlayingMetrics.coverSizeDp.dp)
                .testTag("now-playing-cover"),
            contentAlignment = Alignment.Center,
        ) {
            Box(
                Modifier
                    .size(COVER_SIZE_DP.dp)
                    .testTag("now-playing-scene-cover"),
            )
        }

        // The header leaves the composition as soon as the transition has faded it out
        // of sight: a close or fullscreen button one cannot see must not answer a tap.
        val headerOpacity = 1f - transition
        if (headerOpacity > CONTROL_HIT_EPSILON) {
            PlayedHeader(
                track = track,
                playback = playback,
                surfaceState = surfaceState,
                close = close,
                enterFullscreen = { choose(NowPlayingView.VISUALIZER) },
                opacity = headerOpacity,
            )
        }

        SceneTitle(
            track = track,
            transition = transition,
            controlAlpha = controlAlpha,
            modifier = Modifier
                .align(Alignment.TopCenter)
                .offset(
                    y = (maxHeight * PLAYED_CENTRE_FRACTION + 156.dp) * (1f - transition) +
                        12.dp * transition,
                ),
        )

        SceneProgress(
            track = track,
            playback = playback,
            surfaceState = surfaceState,
            transition = transition,
            controlAlpha = controlAlpha,
            modifier = Modifier
                .align(Alignment.TopCenter)
                .offset(y = maxHeight * 0.69f),
        )

        SceneTransport(
            playback = playback,
            surfaceState = surfaceState,
            transition = transition,
            opacity = controlAlpha,
            leaveFullscreen = { choose(NowPlayingView.PLAYER) },
            wake = { wakeControls() },
            modifier = Modifier
                .align(Alignment.BottomCenter)
                .padding(horizontal = 18.dp, vertical = 18.dp)
                .testTag(
                    if (visualizer && !controlsVisible) {
                        "now-playing-controls-faded"
                    } else {
                        "now-playing-controls-visible"
                    },
                ),
        )

        if (surfaceState.nowPlayingQueueVisible) {
            Box(
                modifier = Modifier
                    .align(Alignment.Center)
                    .size(COVER_SIZE_DP.dp)
                    .background(Color.Black.copy(alpha = 0.9f), RoundedCornerShape(18.dp)),
            ) {
                NowPlayingQueuePage(playback, surfaceState)
            }
        }
    }
}

@Composable
private fun rememberSpectrogram(trackId: Long): SpectrogramFrames {
    val analysis = LocalTrackAnalysis.current
    val revision = analysis.revision
    var frames by remember(trackId) { mutableStateOf<SpectrogramFrames?>(null) }
    DisposableEffect(analysis, trackId, revision) {
        var active = true
        analysis.loadSpectrogram(trackId) { loaded ->
            if (active) frames = loaded
        }
        onDispose { active = false }
    }
    return frames ?: remember(trackId) { SpectrogramFrames(24, 20, ByteArray(0)) }
}

@Composable
private fun PlayedHeader(
    track: LibraryTrack,
    playback: PlaybackUiState,
    surfaceState: MobileSurfaceViewModel,
    close: () -> Unit,
    enterFullscreen: () -> Unit,
    opacity: Float,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 4.dp, vertical = 2.dp)
            .graphicsLayer { alpha = opacity }
            .testTag("now-playing-actions"),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        IconButton(onClick = close, modifier = Modifier.size(44.dp)) {
            MaterialSymbol("keyboard_arrow_down", "Collapse Now Playing")
        }
        Spacer(Modifier.weight(1f))
        QueuePageButton(surfaceState)
        SleepTimerControl(playback.sleepTimer)
        FavouriteHeartButton(track, surfaceState, tag = "now-playing-heart")
        IconButton(
            onClick = enterFullscreen,
            modifier = Modifier
                .size(44.dp)
                .testTag("now-playing-enter-fullscreen"),
        ) {
            MaterialSymbol("fullscreen", "Open fullscreen visualizer")
        }
    }
}

@Composable
private fun SceneTitle(
    track: LibraryTrack,
    transition: Float,
    controlAlpha: Float,
    modifier: Modifier,
) {
    val titleSize = lerp(24f, 33f, transition)
    val artistSize = lerp(13f, 15f, transition)
    Column(
        modifier = modifier
            .fillMaxWidth()
            .padding(horizontal = 28.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text(
            text = track.title,
            modifier = Modifier.testTag("now-playing-title"),
            style = TextStyle(
                fontSize = titleSize.sp,
                lineHeight = (titleSize + 5f).sp,
                fontWeight = if (transition > 0.5f) FontWeight.Light else FontWeight.SemiBold,
            ),
            color = Color.White,
            textAlign = TextAlign.Center,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
        )
        Text(
            text = track.artist.ifBlank { "Unknown artist" },
            style = TextStyle(fontSize = artistSize.sp, fontWeight = FontWeight.Light),
            color = Color.White.copy(alpha = 0.62f * controlAlpha),
            textAlign = TextAlign.Center,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

@Composable
private fun SceneProgress(
    track: LibraryTrack,
    playback: PlaybackUiState,
    surfaceState: MobileSurfaceViewModel,
    transition: Float,
    controlAlpha: Float,
    modifier: Modifier,
) {
    Box(modifier.padding(horizontal = 24.dp)) {
        // Both renderers draw for the whole transition and cross-fade into each other,
        // the way the cover and the burst do. Swapping them at the midpoint would pop,
        // and both are wired to the same scrub state, so either one answers a drag.
        if (transition < 1f) {
            Box(Modifier.graphicsLayer { alpha = 1f - transition }) {
                SpectralSeekSlider(track.id, playback, surfaceState)
            }
        }
        if (transition > 0f) {
            Box(Modifier.graphicsLayer { alpha = transition }) {
                FullscreenProgress(track.id, playback, surfaceState, controlAlpha)
            }
        }
    }
}

@Composable
@OptIn(ExperimentalMaterial3Api::class)
private fun FullscreenProgress(
    trackId: Long,
    playback: PlaybackUiState,
    surfaceState: MobileSurfaceViewModel,
    controlAlpha: Float,
) {
    val seekTo = LocalPlaybackControls.current::seekTo
    val position = surfaceState.seekPosition(trackId, playback.positionMs)
    LaunchedEffect(trackId, playback.positionMs) {
        surfaceState.acceptPlaybackSnapshot(trackId, playback.positionMs)
    }
    val duration = playback.durationMs.coerceAtLeast(0)
    val displayed = position.positionMs.coerceIn(0, duration)
    val maximum = duration.coerceAtLeast(1).toFloat()
    Column(Modifier.fillMaxWidth()) {
        Slider(
            value = displayed.toFloat(),
            onValueChange = { surfaceState.dragTo(trackId, it.toLong()) },
            onValueChangeFinished = { seekTo(surfaceState.releaseScrub(trackId).positionMs) },
            valueRange = 0f..maximum,
            enabled = duration > 0,
            thumb = {
                Box(Modifier.size(12.dp).background(Color.White, CircleShape))
            },
            track = {
                Canvas(Modifier.fillMaxWidth().height(12.dp)) {
                    val fraction = if (duration > 0) displayed.toFloat() / duration else 0f
                    val y = size.height / 2f
                    drawLine(
                        Color.White.copy(alpha = 0.26f),
                        Offset(0f, y),
                        Offset(size.width, y),
                        3.dp.toPx(),
                        StrokeCap.Round,
                    )
                    drawLine(
                        Color.White,
                        Offset(0f, y),
                        Offset(size.width * fraction.coerceIn(0f, 1f), y),
                        3.dp.toPx(),
                        StrokeCap.Round,
                    )
                }
            },
        )
        Row(
            Modifier.fillMaxWidth().alpha(controlAlpha),
            horizontalArrangement = Arrangement.SpaceBetween,
        ) {
            Text(formatDuration(displayed), color = Color.White, fontSize = 13.sp)
            Text(formatRemaining(displayed, duration), color = Color.White, fontSize = 13.sp)
        }
    }
}

@Composable
private fun SceneTransport(
    playback: PlaybackUiState,
    surfaceState: MobileSurfaceViewModel,
    transition: Float,
    opacity: Float,
    leaveFullscreen: () -> Unit,
    wake: () -> Unit,
    modifier: Modifier,
) {
    val controls = LocalPlaybackControls.current
    val fullscreenControls = transition >= 0.5f
    val reachable = opacity > CONTROL_HIT_EPSILON
    Box(modifier.fillMaxWidth()) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .graphicsLayer { alpha = opacity }
                .testTag("now-playing-transport"),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            FlatSceneButton(
                symbol = if (fullscreenControls) "queue_music" else "shuffle",
                description = if (fullscreenControls) "Show queue" else "Toggle shuffle",
                enabled = reachable,
                onClick = if (fullscreenControls) {
                    { surfaceState.showNowPlayingQueue(!surfaceState.nowPlayingQueueVisible) }
                } else {
                    { controls.setShuffle(!playback.shuffled) }
                },
            )
            FlatSceneButton(
                symbol = if (fullscreenControls) "shuffle" else "skip_previous",
                description = if (fullscreenControls) "Toggle shuffle" else "Previous track",
                enabled = reachable,
                onClick = if (fullscreenControls) {
                    { controls.setShuffle(!playback.shuffled) }
                } else {
                    controls::previous
                },
            )
            ScenePauseButton(playback, transition, reachable, controls::togglePause)
            FlatSceneButton(
                symbol = if (fullscreenControls && playback.repeat == AndroidRepeatMode.ONE) {
                    "repeat_one"
                } else if (fullscreenControls) {
                    "repeat"
                } else {
                    "skip_next"
                },
                description = if (fullscreenControls) "Change repeat" else "Next track",
                enabled = reachable,
                onClick = if (fullscreenControls) {
                    { controls.setRepeat(cycleRepeatMode(playback.repeat)) }
                } else {
                    controls::next
                },
            )
            FlatSceneButton(
                symbol = if (fullscreenControls) "fullscreen_exit" else {
                    if (playback.repeat == AndroidRepeatMode.ONE) "repeat_one" else "repeat"
                },
                description = if (fullscreenControls) "Return to player" else "Change repeat",
                enabled = reachable,
                onClick = if (fullscreenControls) {
                    leaveFullscreen
                } else {
                    { controls.setRepeat(cycleRepeatMode(playback.repeat)) }
                },
            )
        }
        if (!reachable) {
            // The faded buttons stay mounted, and a disabled `clickable` still wins the
            // hit test instead of letting the touch through to the scene below. Without
            // this catcher the transport row would be the one part of the screen whose
            // tap cannot bring the controls back — and it is the likeliest place to tap.
            Box(
                modifier = Modifier
                    .matchParentSize()
                    .testTag("now-playing-controls-wake")
                    .clickable(
                        interactionSource = remember { MutableInteractionSource() },
                        indication = null,
                        onClick = wake,
                    ),
            )
        }
    }
}

@Composable
private fun FlatSceneButton(
    symbol: String,
    description: String,
    enabled: Boolean,
    onClick: () -> Unit,
) {
    IconButton(onClick = onClick, enabled = enabled, modifier = Modifier.size(48.dp)) {
        MaterialSymbol(symbol, description, tint = Color.White)
    }
}

@Composable
private fun ScenePauseButton(
    playback: PlaybackUiState,
    transition: Float,
    enabled: Boolean,
    onClick: () -> Unit,
) {
    val size = lerp(80f, 62f, transition)
    val radius = lerp(28f, 31f, transition)
    val shape = RoundedCornerShape(radius.dp)
    IconButton(
        onClick = onClick,
        enabled = enabled,
        modifier = Modifier
            .size(size.dp)
            .testTag("now-playing-play")
            .clip(shape)
            .background(MaterialTheme.colorScheme.primary.copy(alpha = 1f - transition))
            .border(1.6.dp, Color.White.copy(alpha = transition), shape),
    ) {
        MaterialSymbol(
            name = if (playback.isPlaying) "pause" else "play_arrow",
            contentDescription = playback.playPauseLabel,
            tint = Color.White,
            sizeSp = lerp(40f, 34f, transition).roundToInt(),
        )
    }
}

private fun DrawScope.drawPlayedCover(
    artwork: ImageBitmap?,
    center: Offset,
    fallback: Color,
    opacity: Float,
    scale: Float,
) {
    if (opacity <= 0f) return
    val side = COVER_SIZE_DP.dp.toPx() * scale
    val rect = Rect(
        center.x - side / 2f,
        center.y - side / 2f,
        center.x + side / 2f,
        center.y + side / 2f,
    )
    val radius = COVER_RADIUS_DP.dp.toPx() * scale
    repeat(5) { index ->
        val spread = index * 2.dp.toPx()
        drawRoundRect(
            color = Color.Black.copy(alpha = opacity * (0.24f - index * 0.035f)),
            topLeft = Offset(rect.left - spread, rect.top + (12 + index * 3).dp.toPx()),
            size = Size(rect.width + spread * 2f, rect.height + spread),
            cornerRadius = CornerRadius(radius + spread),
        )
    }
    val path = Path().apply { addRoundRect(RoundRect(rect, CornerRadius(radius))) }
    clipPath(path) {
        if (artwork == null) {
            drawRect(fallback.copy(alpha = opacity), topLeft = rect.topLeft, size = rect.size)
        } else {
            drawImage(
                image = artwork,
                dstOffset = IntOffset(rect.left.roundToInt(), rect.top.roundToInt()),
                dstSize = IntSize(side.roundToInt(), side.roundToInt()),
                alpha = opacity,
            )
        }
    }
}

private fun lerp(start: Float, end: Float, fraction: Float): Float =
    start + (end - start) * fraction.coerceIn(0f, 1f)

/** Keeps the frame counter captured by the scene's draw lambda; the value is not drawn. */
private fun observeSceneFrame(@Suppress("UNUSED_PARAMETER") revision: Int) = Unit
