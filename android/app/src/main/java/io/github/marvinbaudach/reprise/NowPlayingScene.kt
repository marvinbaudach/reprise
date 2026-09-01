package io.github.marvinbaudach.reprise

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
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
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.FrameRateCategory
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.RoundRect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.clipPath
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.preferredFrameRate
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.marvinbaudach.reprise.scene.SceneState
import io.github.marvinbaudach.reprise.scene.SpectrogramFrames
import io.github.marvinbaudach.reprise.ui.theme.AmbientTrueBlack
import io.github.marvinbaudach.reprise.ui.theme.NowPlayingOnBackdrop
import io.github.marvinbaudach.reprise.ui.theme.toComposeColor
import uniffi.reprise_android_ffi.AndroidArtworkSize
import uniffi.reprise_android_ffi.AndroidRepeatMode
import kotlin.math.roundToInt

private const val COVER_SIZE_DP = 272
private const val COVER_RADIUS_DP = 18f
private const val PLAYED_CENTRE_FRACTION = 0.34f
private const val TITLE_TO_ARTIST_GAP_DP = 6

internal fun shouldRequestHighVisualizerFrameRate(
    visualizerOpacity: Float,
    playing: Boolean,
): Boolean = visualizerOpacity.isFinite() && visualizerOpacity > 0f && playing

internal fun requestedVisualizerFrameRateCategory(
    visualizerOpacity: Float,
    playing: Boolean,
): FrameRateCategory? = if (shouldRequestHighVisualizerFrameRate(visualizerOpacity, playing)) {
    FrameRateCategory.High
} else {
    null
}

@Composable
internal fun NowPlayingScene(
    track: LibraryTrack,
    playback: PlaybackUiState,
    surfaceState: MobileSurfaceViewModel,
    horizontalOffsetPx: Float = 0f,
    previousTrack: LibraryTrack? = null,
    nextTrack: LibraryTrack? = null,
    visualizerOpacity: Float = 0f,
    onCoverBounds: (Rect) -> Unit = {},
) {
    val frames = rememberSpectrogram(track.id)
    val state = remember(frames) { SceneState(frames) }
    val artwork = rememberTrackArtworkVisual(
        track.uri,
        AndroidArtworkSize.NOW_PLAYING,
        track.title,
        track.artist,
    )
    val previousArtwork = previousTrack?.let { neighbour ->
        rememberTrackArtworkVisual(
            neighbour.uri,
            AndroidArtworkSize.NOW_PLAYING,
            neighbour.title,
            neighbour.artist,
        )
    }
    val nextArtwork = nextTrack?.let { neighbour ->
        rememberTrackArtworkVisual(
            neighbour.uri,
            AndroidArtworkSize.NOW_PLAYING,
            neighbour.title,
            neighbour.artist,
        )
    }
    val fog = rememberCoverFogTransition(artwork?.image, AmbientTrueBlack)
    val coverShadow = rememberCoverShadowBitmap()
    val motion = LocalAmbientMotionController.current
    val fallbackAccent = MaterialTheme.colorScheme.primary
    val visualEngine = rememberVisualSceneEngine(
        trackId = track.id,
        playback = playback,
        accent = artwork?.ambientColors?.first?.toComposeColor() ?: fallbackAccent,
    )
    val visualFrameSink = remember(visualEngine) { visualEngine?.let(::visualSceneFrameSink) }
    val drawRevision = DriveScene(
        frames = frames,
        state = state,
        playback = playback,
        controller = motion,
        frameSink = visualFrameSink,
    )
    val power = motion.sceneRenderPower()

    BoxWithConstraints(
        modifier = Modifier
            .fillMaxSize()
            .background(AmbientTrueBlack)
            .testTag("now-playing-player")
            .then(
                requestedVisualizerFrameRateCategory(
                    visualizerOpacity,
                    playback.visualizerActive,
                )?.let { category -> Modifier.preferredFrameRate(category) } ?: Modifier,
            ),
    ) {
        val density = LocalDensity.current
        val coverTop = maxHeight * PLAYED_CENTRE_FRACTION - (COVER_SIZE_DP / 2).dp
        val reportedCoverBounds = with(density) {
            playedCoverRect(
                center = Offset(
                    maxWidth.toPx() / 2f + horizontalOffsetPx,
                    maxHeight.toPx() * PLAYED_CENTRE_FRACTION,
                ),
                side = COVER_SIZE_DP.dp.toPx(),
            )
        }
        SideEffect { onCoverBounds(reportedCoverBounds) }
        Canvas(
            modifier = Modifier
                .fillMaxSize()
                .testTag("now-playing-scene"),
        ) {
            // Capturing the frame counter is what makes Compose re-run this lambda once
            // per scene frame; the value itself has nothing to contribute to the drawing.
            observeSceneFrame(drawRevision)
            drawRect(AmbientTrueBlack)
            val playedCenter = Offset(
                size.width / 2f + horizontalOffsetPx,
                size.height * PLAYED_CENTRE_FRACTION,
            )
            val fogCenter = playedCenter.copy(
                x = size.width / 2f + horizontalOffsetPx * FOG_SWIPE_DISTANCE_FACTOR,
            )
            // The film is drawn first and lit from whichever picture the cover
            // slot is currently showing — the artwork's own quadrants, or the
            // visualizer's ramp once the spectrum has crossed over it.
            drawPlayedNowPlayingFog(
                fog = fog.previous,
                center = fogCenter,
                state = state,
                visualizerOpacity = visualizerOpacity,
                opacity = 1f - fog.fraction,
                rotationsEnabled = power.fogRotates,
            )
            drawPlayedNowPlayingFog(
                fog = fog.current,
                center = fogCenter,
                state = state,
                visualizerOpacity = visualizerOpacity,
                opacity = fog.fraction,
                rotationsEnabled = power.fogRotates,
            )
            drawPlayedNowPlayingShimmer(
                fog = fog.previous,
                center = fogCenter,
                state = state,
                opacity = 1f - fog.fraction,
                rotationsEnabled = power.fogRotates,
            )
            drawPlayedNowPlayingShimmer(
                fog = fog.current,
                center = fogCenter,
                state = state,
                opacity = fog.fraction,
                rotationsEnabled = power.fogRotates,
            )
            if (horizontalOffsetPx > 0f) {
                previousArtwork?.image?.let { neighbour ->
                    drawPlayedCover(
                        artwork = neighbour,
                        center = playedCenter.copy(x = playedCenter.x - size.width),
                        fallback = AmbientTrueBlack,
                        shadow = coverShadow,
                    )
                }
            } else if (horizontalOffsetPx < 0f) {
                nextArtwork?.image?.let { neighbour ->
                    drawPlayedCover(
                        artwork = neighbour,
                        center = playedCenter.copy(x = playedCenter.x + size.width),
                        fallback = AmbientTrueBlack,
                        shadow = coverShadow,
                    )
                }
            }
            drawPlayedCover(
                artwork = artwork?.image,
                center = playedCenter,
                fallback = AmbientTrueBlack,
                shadow = coverShadow,
                opacity = 1f - visualizerOpacity,
            )
            if (visualEngine != null && visualizerOpacity > 0f) {
                drawPlayedVisualizer(
                    buffer = visualEngine.sceneBytes(
                        width = COVER_SIZE_DP.dp.toPx(),
                        height = COVER_SIZE_DP.dp.toPx(),
                    ),
                    center = playedCenter,
                    side = COVER_SIZE_DP.dp.toPx(),
                    radius = COVER_RADIUS_DP.dp.toPx(),
                    shadow = null,
                    opacity = visualizerOpacity,
                )
            }
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

        PlayedHeader(track = track, playback = playback, surfaceState = surfaceState)

        SceneTitle(
            track = track,
            modifier = Modifier
                .align(Alignment.TopCenter)
                .offset(y = maxHeight * PLAYED_CENTRE_FRACTION + 156.dp),
        )

        SceneProgress(
            track = track,
            playback = playback,
            surfaceState = surfaceState,
            modifier = Modifier
                .align(Alignment.TopCenter)
                .offset(y = maxHeight * 0.69f),
        )

        SceneTransport(
            playback = playback,
            modifier = Modifier
                .align(Alignment.BottomCenter)
                .padding(horizontal = 18.dp, vertical = 18.dp),
        )
    }
}

@Composable
private fun rememberVisualSceneEngine(
    trackId: Long,
    playback: PlaybackUiState,
    accent: Color,
): VisualSceneEngine? {
    val factory = LocalVisualSceneEngineFactory.current
    val engine: VisualSceneEngine? = remember(factory) { factory.create() }
    DisposableEffect(engine) {
        onDispose { engine?.close() }
    }
    DisposableEffect(engine, trackId) {
        engine?.noteTrackChanged()
        onDispose { }
    }
    SideEffect {
        engine?.let { updateVisualSceneEngine(it, playback, accent) }
    }
    return engine
}

internal fun updateVisualSceneEngine(
    engine: VisualSceneEngine,
    playback: PlaybackUiState,
    accent: Color,
) {
    engine.setPlaying(playback.visualizerActive)
    engine.setAccent(accent.red, accent.green, accent.blue)
}

internal fun visualSceneFrameSink(engine: VisualSceneEngine): SceneFrameSink =
    object : SceneFrameSink {
        override fun hasLiveAudio(): Boolean = engine.hasLiveAudio()

        override fun bassPressure(): VisualBassPressure = engine.bassPressure()

        override fun onFrame(bands: FloatArray?) {
            if (bands != null) engine.ingestBands(bands)
            engine.tick()
        }
    }

/** The played-view wiring kept shared with its rendered-pixel verification. */
internal fun DrawScope.drawPlayedNowPlayingFog(
    fog: CoverFogBitmap?,
    center: Offset,
    state: SceneState,
    visualizerOpacity: Float,
    opacity: Float,
    rotationsEnabled: Boolean,
) {
    drawNowPlayingFog(
        // Behind the spectrum there is no artwork to read a palette from, so
        // the film borrows the ramp the bars themselves are drawn from and
        // follows the cross-fade across to it.
        palette = fog?.palette?.blendedTo(VisualizerRampPalette, visualizerOpacity),
        center = center,
        seconds = state.oilFilmSeconds,
        level = state.oilFilmLevel,
        opacity = opacity,
        driftEnabled = rotationsEnabled,
    )
}

/** The cover-disc wiring shared with its deterministic renderer tests. */
internal fun DrawScope.drawPlayedNowPlayingShimmer(
    fog: CoverFogBitmap?,
    center: Offset,
    state: SceneState,
    opacity: Float,
    rotationsEnabled: Boolean,
) {
    drawNowPlayingShimmer(
        fog = fog,
        center = center,
        coverDiameterDp = COVER_SIZE_DP.toFloat(),
        elapsedSeconds = state.shimmerElapsedSeconds,
        swell = state.fogLevel,
        opacity = opacity,
        rotationsEnabled = rotationsEnabled,
    )
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
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 4.dp, vertical = 2.dp)
            .testTag("now-playing-actions"),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Spacer(Modifier.weight(1f))
        SleepTimerControl(playback.sleepTimer)
        FavouriteHeartButton(
            track,
            surfaceState,
            tag = "now-playing-heart",
            enabled = LocalNowPlayingActionsEnabled.current,
        )
        // The fullscreen visualizer this row used to open is retired, so the
        // context menu takes the slot rather than sitting next to it.
        NowPlayingTrackContextMenu(track)
    }
}

@Composable
private fun SceneTitle(
    track: LibraryTrack,
    modifier: Modifier,
) {
    Column(
        modifier = modifier
            .fillMaxWidth()
            .padding(horizontal = 28.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        // The title takes the height it needs rather than always reserving two
        // lines: the reservation left a visible hole under every one-line title,
        // and it was buying less than it looked. Everything below this block —
        // seek bar, transport — is placed against the screen height, so a title
        // growing to a second line moves the artist line and nothing else.
        Text(
            text = track.title,
            modifier = Modifier.testTag("now-playing-title"),
            style = TextStyle(
                fontSize = 24.sp,
                lineHeight = 29.sp,
                fontWeight = FontWeight.SemiBold,
            ),
            color = NowPlayingOnBackdrop,
            textAlign = TextAlign.Center,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
        )
        Spacer(Modifier.height(TITLE_TO_ARTIST_GAP_DP.dp))
        Text(
            text = track.artist.ifBlank { "Unknown artist" },
            style = TextStyle(fontSize = 13.sp, fontWeight = FontWeight.Light),
            color = NowPlayingOnBackdrop.copy(alpha = 0.62f),
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
    modifier: Modifier,
) {
    Box(modifier.padding(horizontal = 24.dp)) {
        SpectralSeekSlider(track.id, playback, surfaceState)
    }
}

@Composable
private fun SceneTransport(
    playback: PlaybackUiState,
    modifier: Modifier,
) {
    val controls = LocalPlaybackControls.current
    Row(
        modifier = modifier
            .fillMaxWidth()
            .testTag("now-playing-transport"),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        FlatSceneButton(
            symbol = "shuffle",
            description = if (playback.shuffled) "Turn shuffle off" else "Turn shuffle on",
            active = playback.shuffled,
            tag = "now-playing-shuffle",
            onClick = { controls.setShuffle(!playback.shuffled) },
        )
        FlatSceneButton("skip_previous", "Previous track", onClick = controls::previous)
        ScenePauseButton(playback, controls::togglePause)
        FlatSceneButton("skip_next", "Next track", onClick = controls::next)
        FlatSceneButton(
            symbol = if (playback.repeat == AndroidRepeatMode.ONE) "repeat_one" else "repeat",
            description = "Repeat ${playback.repeat.name.lowercase()}",
            active = playback.repeat != AndroidRepeatMode.OFF,
            tag = "now-playing-repeat",
            onClick = { controls.setRepeat(cycleRepeatMode(playback.repeat)) },
        )
    }
}

@Composable
private fun FlatSceneButton(
    symbol: String,
    description: String,
    active: Boolean = false,
    tag: String? = null,
    onClick: () -> Unit,
) {
    IconButton(
        onClick = onClick,
        modifier = Modifier
            .size(48.dp)
            .then(if (tag == null) Modifier else Modifier.testTag(tag))
            .semantics { selected = active }
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
            symbol,
            description,
            tint = if (active) {
                MaterialTheme.colorScheme.onSecondaryContainer
            } else {
                NowPlayingOnBackdrop
            },
        )
    }
}

@Composable
private fun ScenePauseButton(
    playback: PlaybackUiState,
    onClick: () -> Unit,
) {
    val shape = RoundedCornerShape(28.dp)
    IconButton(
        onClick = onClick,
        modifier = Modifier
            .size(80.dp)
            .testTag("now-playing-play")
            .clip(shape)
            .background(MaterialTheme.colorScheme.primary),
    ) {
        MaterialSymbol(
            name = playback.playPauseSymbol,
            contentDescription = playback.playPauseLabel,
            tint = NowPlayingOnBackdrop,
            sizeSp = 40,
        )
    }
}

internal fun DrawScope.drawPlayedCover(
    artwork: ImageBitmap?,
    center: Offset,
    fallback: Color,
    shadow: CoverShadowBitmap?,
    opacity: Float = 1f,
) {
    val side = COVER_SIZE_DP.dp.toPx()
    val rect = playedCoverRect(center, side)
    val radius = COVER_RADIUS_DP.dp.toPx()
    shadow?.let { drawCoverShadow(it, rect) }
    if (opacity <= 0f) return
    val safeOpacity = opacity.coerceIn(0f, 1f)
    val path = Path().apply { addRoundRect(RoundRect(rect, CornerRadius(radius))) }
    clipPath(path) {
        if (artwork == null) {
            drawRect(
                color = fallback.copy(alpha = fallback.alpha * safeOpacity),
                topLeft = rect.topLeft,
                size = rect.size,
            )
        } else {
            drawImage(
                image = artwork,
                dstOffset = IntOffset(rect.left.roundToInt(), rect.top.roundToInt()),
                dstSize = IntSize(side.roundToInt(), side.roundToInt()),
                alpha = safeOpacity,
            )
        }
    }
}

internal fun playedCoverRect(center: Offset, side: Float): Rect = Rect(
    center.x - side / 2f,
    center.y - side / 2f,
    center.x + side / 2f,
    center.y + side / 2f,
)

/** Keeps the frame counter captured by the scene's draw lambda; the value is not drawn. */
private fun observeSceneFrame(@Suppress("UNUSED_PARAMETER") revision: Int) = Unit

private const val FOG_SWIPE_DISTANCE_FACTOR = 0.35f
