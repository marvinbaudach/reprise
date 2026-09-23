package io.github.marvinbaudach.reprise

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
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
import androidx.compose.foundation.layout.requiredWidth
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.blur
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.RoundRect
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.graphics.ColorMatrix
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.clipPath
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.preferredFrameRate
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
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
import kotlin.math.roundToInt
import kotlin.math.abs
import kotlin.math.max
import kotlin.math.min

// Shared with NowPlayingFogLayer.kt, which centres the one stationary fog
// canvas on the same cover geometry the panels use.
internal const val COVER_SIZE_DP = 272
private const val COVER_RADIUS_DP = 18f
internal const val PLAYED_CENTRE_FRACTION = 0.34f
private const val TITLE_TO_ARTIST_GAP_DP = 6
private const val MAXIMUM_COLOR_CHANNEL = 255

private val SATURATION_FILTERS by lazy {
    Array(MAXIMUM_COLOR_CHANNEL + 1) { channel ->
        val saturation = channel.toFloat() / MAXIMUM_COLOR_CHANNEL
        ColorFilter.colorMatrix(ColorMatrix().apply { setToSaturation(saturation) })
    }
}

private fun cachedSaturationFilter(saturation: Float): ColorFilter? {
    if (saturation.toRawBits() == 1f.toRawBits()) return null
    val channel = (saturation.coerceIn(0f, 1f) * MAXIMUM_COLOR_CHANNEL).roundToInt()
    return SATURATION_FILTERS[channel]
}

@Composable
internal fun NowPlayingScene(
    track: LibraryTrack,
    playback: PlaybackUiState,
    surfaceState: MobileSurfaceViewModel,
    positionPx: Float = 0f,
    currentIndex: Int = 0,
    panels: List<PlayPanel> = listOf(PlayPanel(currentIndex, track)),
    visualizerOpacity: Float = 0f,
    visualizerLight: Float = visualizerOpacity,
    cueRevision: Int = 0,
    // Hoisted with a default rather than remembered locally so a test can hand
    // in its own handle and read what the live panel published to it.
    liveScene: LiveSceneHandle = remember { LiveSceneHandle() },
    onCoverBounds: (Rect) -> Unit = {},
    onSeekBounds: (Rect) -> Unit = {},
    onPrevious: () -> Unit = {},
    onNext: () -> Unit = {},
) {
    val motion = LocalAmbientMotionController.current

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
        val widthPx = with(density) { maxWidth.toPx() }
        val currentTransform = nowPlayingPanelTransform(currentIndex, positionPx, widthPx)
        val progressTransform = nowPlayingProgressTransform(currentIndex, positionPx, widthPx)
        val coverTop = maxHeight * PLAYED_CENTRE_FRACTION - (COVER_SIZE_DP / 2).dp
        val titleTop = maxHeight * PLAYED_CENTRE_FRACTION + 156.dp
        val displayWidth = maxWidth
        val titleWidth = maxWidth * TITLE_PANEL_WIDTH_RATIO
        val reportedCoverBounds = with(density) {
            playedCoverRect(
                center = Offset(
                    maxWidth.toPx() / 2f + currentTransform.translationX,
                    maxHeight.toPx() * PLAYED_CENTRE_FRACTION,
                ),
                side = COVER_SIZE_DP.dp.toPx(),
            )
        }
        SideEffect { onCoverBounds(reportedCoverBounds) }
        Box(Modifier.fillMaxSize().testTag("now-playing-scene")) {
            // Drawn first so every panel rides on top of it: the fog no longer
            // belongs to any one panel's canvas (see NowPlayingPanelLayer below),
            // it is one stationary layer the live panel publishes into.
            NowPlayingFogLayer(liveScene, motion, visualizerLight)
            panels.forEach { panel ->
                key(panel.track.id, panel.index) {
                    NowPlayingPanelLayer(
                        panel = panel,
                        currentIndex = currentIndex,
                        positionPx = positionPx,
                        widthPx = widthPx,
                        playback = playback,
                        motion = motion,
                        visualizerOpacity = visualizerOpacity,
                        coverTop = coverTop,
                        liveScene = liveScene,
                    )
                    SceneTitle(
                        track = panel.track,
                        displayWidth = displayWidth,
                        modifier = Modifier
                            .align(Alignment.TopStart)
                            .offset(y = titleTop)
                            .requiredWidth(titleWidth)
                            .graphicsLayer {
                                translationX = nowPlayingTitleTranslation(
                                    positionPx = positionPx - panel.index * widthPx,
                                )
                                alpha = max(
                                    0f,
                                    1f - abs(panel.index - positionPx / widthPx) * 1.35f,
                                )
                            },
                    )
                }
            }
        }

        Box(
            modifier = Modifier
                .align(Alignment.TopCenter)
                .offset(y = coverTop - ((nowPlayingMetrics.coverSizeDp - COVER_SIZE_DP) / 2).dp)
                .size(nowPlayingMetrics.coverSizeDp.dp)
                .graphicsLayer { translationX = currentTransform.translationX }
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

        SceneProgress(
            track = track,
            playback = playback,
            surfaceState = surfaceState,
            cueRevision = cueRevision,
            animationsEnabled = motion.sceneAnimationsEnabled,
            transform = progressTransform,
            onSeekBounds = onSeekBounds,
            modifier = Modifier
                .align(Alignment.TopCenter)
                .offset(y = maxHeight * 0.69f),
        )

        SceneTransport(
            playback = playback,
            cueRevision = cueRevision,
            animationsEnabled = motion.sceneAnimationsEnabled,
            onPrevious = onPrevious,
            onNext = onNext,
            modifier = Modifier
                .align(Alignment.BottomCenter)
                .padding(horizontal = 18.dp, vertical = 18.dp),
        )
    }
}

@Composable
private fun NowPlayingPanelLayer(
    panel: PlayPanel,
    currentIndex: Int,
    positionPx: Float,
    widthPx: Float,
    playback: PlaybackUiState,
    motion: AmbientMotionController,
    visualizerOpacity: Float,
    coverTop: androidx.compose.ui.unit.Dp,
    liveScene: LiveSceneHandle,
) {
    val artwork = rememberTrackArtworkVisual(
        panel.track.uri,
        AndroidArtworkSize.NOW_PLAYING,
        panel.track.title,
        panel.track.artist,
        allowFetch = true,
    )
    val fog = rememberCoverFogBitmap(artwork?.image, AmbientTrueBlack)
    val frames = rememberSpectrogram(panel.track.id)
    val state = remember(frames) { SceneState(frames) }
    val accent = artwork?.ambientColors?.first?.toComposeColor()
        ?: MaterialTheme.colorScheme.primary
    val isLivePanel = panel.index == currentIndex
    val visualEngine = rememberVisualSceneEngine(
        panel.track.id,
        playback,
        accent,
        live = isLivePanel,
        liveScene = liveScene,
    )
    // Read via rememberUpdatedState, not the plain val: this panel can keep the
    // live slot across many recompositions in which fog/state are replaced
    // (a fresh blur landing, a track change swapping the frames), while the
    // DisposableEffect below only remounts when visualEngine's identity
    // changes. Its onDispose must still see whatever this panel last
    // published, or the === guard below would compare against a stale value
    // and never clear the handle.
    val latestFog by rememberUpdatedState(fog)
    val latestState by rememberUpdatedState(state)
    if (isLivePanel) {
        DisposableEffect(liveScene, visualEngine) {
            liveScene.engine = visualEngine
            onDispose {
                if (liveScene.engine === visualEngine) liveScene.engine = null
                if (liveScene.fog === latestFog) liveScene.fog = null
                if (liveScene.state === latestState) liveScene.state = null
            }
        }
    }
    val frameSink = remember(visualEngine) { visualEngine?.let(::visualSceneFrameSink) }
    val drawRevision = DriveScene(frames, state, playback, motion, frameSink)
    if (isLivePanel) {
        // The revision write is what invalidates NowPlayingFogLayer's canvas:
        // it is the one field here that changes every scene frame, and the
        // layer's draw lambda reads it purely so Compose reruns that draw,
        // the same way each panel's own canvases already observe it.
        SideEffect {
            liveScene.fog = fog
            liveScene.state = state
            liveScene.drawRevision = drawRevision
        }
    }
    val transform = nowPlayingPanelTransform(panel.index, positionPx, widthPx)
    val distance = if (widthPx > 0f) abs(panel.index - positionPx / widthPx) else 0f
    val near = max(0f, 1f - min(1f, distance))
    val frozenScene = rememberFrozenSceneBytes(panel.track.id)
    val canMirrorLiveScene = panelCanMirrorLiveScene(
        isLivePanel,
        frames.frameCount,
        liveSceneAvailable = liveScene.engine != null,
    )
    val mirroredEngine = liveScene.engine.takeIf {
        panelMirrorsLiveScene(isLivePanel, frames.frameCount, near, liveSceneAvailable = it != null)
    }
    val hasVisualData = panelHasVisualData(
        frames.frameCount,
        frozenScene.hasCapturedScene,
        canMirrorLiveScene = canMirrorLiveScene,
    )
    val dataAvailability by animateFloatAsState(
        targetValue = if (hasVisualData) 1f else 0f,
        // Shares its timing with the visualizerOpacity toggle for a matching feel, not because it
        // is the same event: this fades one panel's data arriving, not the cover/visualizer mode.
        animationSpec = tween(VISUALIZER_CROSSFADE_MS),
        label = "now playing panel data availability",
    )
    val blend = nowPlayingVisualBlend(visualizerOpacity, dataAvailability)
    val coverOpacity = blend.coverOpacity
    val barsOpacity = blend.barsOpacity
    val barHeight = 0.3f + near * 0.7f
    val coverShadow = rememberCoverShadowBitmap()
    val density = LocalDensity.current
    val saturationFilter = cachedSaturationFilter(transform.saturation)

    // The panel now draws only its own cover box and bars; the fog is
    // NowPlayingFogLayer's, drawn once behind every panel (see NowPlayingScene).
    Box(
        modifier = Modifier
            .offset(y = coverTop)
            .fillMaxWidth()
            .height(COVER_SIZE_DP.dp)
            .graphicsLayer {
                translationX = transform.translationX
                scaleX = transform.scale
                scaleY = transform.scale
                transform.rotationForLayer?.let { rotationZ = it }
                alpha = transform.opacity
                colorFilter = saturationFilter
            }
            .then(
                if (transform.blurPx.toRawBits() == 0f.toRawBits()) {
                    Modifier
                } else {
                    Modifier.blur(with(density) { transform.blurPx.toDp() })
                },
            ),
        contentAlignment = Alignment.Center,
    ) {
        Canvas(Modifier.size(COVER_SIZE_DP.dp)) {
            observeSceneFrame(drawRevision)
            val center = Offset(size.width / 2f, size.height / 2f)
            drawPlayedCover(
                artwork = artwork?.image,
                center = center,
                fallback = AmbientTrueBlack,
                shadow = coverShadow,
                opacity = coverOpacity,
            )
        }
        val awaitingFirstLiveScene = panelAwaitsFirstLiveScene(
            visualizerOpacity,
            drawsLiveScene = isLivePanel || mirroredEngine != null,
            frozenScene.hasCapturedScene,
        )
        // A resting neighbour sits off the screen: it draws no bars at all, so
        // the frozen picture it keeps for the next swipe costs nothing per frame.
        val onScreen = isLivePanel || near > 0f
        if (visualEngine != null && onScreen && (barsOpacity > 0f || awaitingFirstLiveScene)) {
            Canvas(
                Modifier
                    .size(COVER_SIZE_DP.dp)
                    .graphicsLayer { scaleY = barHeight },
            ) {
                observeSceneFrame(drawRevision)
                val center = Offset(size.width / 2f, size.height / 2f)
                val scene = when {
                    mirroredEngine != null -> mirroredEngine.sceneBytesTinted(
                        size.width,
                        size.height,
                        accent.red,
                        accent.green,
                        accent.blue,
                    )
                    isLivePanel || frames.frameCount > 0 -> visualEngine.sceneBytes(size.width, size.height)
                    else -> ByteArray(0)
                }
                drawPlayedVisualizer(
                    buffer = frozenScene.latestOrFrozen(scene),
                    center = center,
                    side = size.width,
                    radius = COVER_RADIUS_DP.dp.toPx(),
                    shadow = null,
                    opacity = barsOpacity,
                )
            }
        }
    }
}

@Composable
private fun rememberVisualSceneEngine(
    trackId: Long,
    playback: PlaybackUiState,
    accent: Color,
    live: Boolean,
    liveScene: LiveSceneHandle,
): VisualSceneEngine? {
    val factory = visualSceneFactoryForPanel(live, LocalVisualSceneEngineFactory.current)
    // Read before the engine swap below: while the panel that is losing the
    // live slot is still composing this same pass, `liveScene.engine` is
    // still its outgoing engine — its DisposableEffect that would null this
    // out has not run yet (see `shouldAdoptLiveShape`).
    val previousLiveEngine = liveScene.engine
    val engine: VisualSceneEngine? = remember(factory) { factory.create() }
    // Read alongside engine creation, not inside the adopt effect below: by
    // the time effects run, the outgoing panel's own `DisposableEffect(engine)
    // { onDispose { engine?.close() } }` may already have closed
    // `previousLiveEngine`, and reading a closed native engine throws.
    // This native read deliberately happens during composition, before that outgoing lease closes.
    // It is idempotent, for the same reason `factory.create()` belongs in this `remember` block.
    val adoptedBands = remember(factory) {
        val created = engine
        if (created != null && shouldAdoptLiveShape(live, previousLiveEngine, created)) {
            previousLiveEngine!!.currentBands()
        } else {
            null
        }
    }
    DisposableEffect(engine) {
        onDispose { engine?.close() }
    }
    DisposableEffect(engine, trackId) {
        engine?.noteTrackChanged()
        onDispose { }
    }
    // Declared after the `noteTrackChanged` effect above: that call clears
    // `has_ingested` on the Rust side, which would otherwise wipe the shape
    // this adopts right back out.
    DisposableEffect(engine) {
        adoptedBands?.let { engine?.adoptShape(it) }
        onDispose { }
    }
    SideEffect {
        engine?.let { updateVisualSceneEngine(it, playback, accent) }
    }
    return engine
}

internal fun visualSceneFactoryForPanel(
    live: Boolean,
    liveFactory: VisualSceneEngineFactory,
): VisualSceneEngineFactory = if (live) liveFactory else NativeVisualSceneEngineFactory

internal fun updateVisualSceneEngine(
    engine: VisualSceneEngine,
    playback: PlaybackUiState,
    accent: Color,
) {
    engine.setPlaying(playback.visualizerActive)
    engine.setAccent(accent.red, accent.green, accent.blue)
}

/**
 * What the live panel publishes for the rest of the scene to read.
 *
 * [engine] is read by a neighbour the swipe has carried onto the screen (see
 * [panelMirrorsLiveScene]) — only ever the current engine or null, and a
 * neighbour never keeps it past the frame it drew. [fog] and [state] are read
 * by [NowPlayingFogLayer], the one stationary fog canvas behind every panel:
 * since the fog no longer belongs to any one panel's own canvas, it has to
 * learn what the live panel is currently showing from here instead.
 * [drawRevision] is the live panel's own per-frame counter ([DriveScene]'s
 * return value), republished so the fog layer's canvas invalidates on exactly
 * the same frames the live panel's own canvases do.
 */
internal class LiveSceneHandle {
    var engine: VisualSceneEngine? by mutableStateOf(null)
    var fog: CoverFogBitmap? by mutableStateOf(null)
    var state: SceneState? by mutableStateOf(null)
    var drawRevision: Int by mutableIntStateOf(0)
}

@Composable
private fun rememberFrozenSceneBytes(trackId: Long): FrozenSceneBytes =
    remember(trackId) { FrozenSceneBytes() }

/**
 * Keeps a panel's last drawn scene buffer alive across its own engine swaps.
 *
 * A panel's [VisualSceneEngine] is replaced the moment it crosses into (or
 * out of) the live slot — see [visualSceneFactoryForPanel] — and a freshly
 * live engine reports an empty scene ([VisualSceneEngine.sceneBytes]) until
 * it has ingested its first frame (`has_ingested` on the Rust side). Keyed on
 * the panel's own track rather than on the engine, this instance survives
 * exactly that swap and hands back the panel's last non-empty picture in the
 * gap, so the transition never draws nothing.
 */
internal class FrozenSceneBytes {
    private var lastNonEmpty: ByteArray = ByteArray(0)

    /**
     * True once a real, non-empty scene has been drawn at least once.
     *
     * Used to decide whether a live panel is allowed to show bars at all: a
     * track with no stored spectrogram starts with nothing to scene, and this
     * stays false — keeping the cover up — until live audio produces the first
     * real frame.
     */
    val hasCapturedScene: Boolean
        get() = lastNonEmpty.isNotEmpty()

    fun latestOrFrozen(scene: ByteArray): ByteArray {
        if (scene.isNotEmpty()) lastNonEmpty = scene
        return lastNonEmpty
    }
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
    displayWidth: Dp,
    modifier: Modifier,
) {
    // The block this sits in is deliberately wider than the display — that
    // surplus is what lets the title travel faster than the cover during a
    // swipe. The text must not inherit it: laid out against the panel width a
    // long title still fits, so its ellipsis never fires and the glyphs run off
    // both edges of the screen. The block keeps the parallax, this column keeps
    // the display's own width.
    Box(modifier = modifier, contentAlignment = Alignment.TopCenter) {
        Column(
            modifier = Modifier
                .width(displayWidth)
                .padding(horizontal = 28.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            // The title takes the height it needs rather than always reserving
            // two lines: the reservation left a visible hole under every
            // one-line title, and it was buying less than it looked. Everything
            // below this block — seek bar, transport — is placed against the
            // screen height, so a title growing to a second line moves the
            // artist line and nothing else.
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
                modifier = Modifier.testTag("now-playing-artist"),
                style = TextStyle(fontSize = 13.sp, fontWeight = FontWeight.Light),
                color = NowPlayingOnBackdrop.copy(alpha = 0.62f),
                textAlign = TextAlign.Center,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
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
    if (opacity <= 0f) return
    val safeOpacity = opacity.coerceIn(0f, 1f)
    shadow?.let {
        drawCoverShadow(it, rect, alpha = safeOpacity)
    }
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
internal fun observeSceneFrame(@Suppress("UNUSED_PARAMETER") revision: Int) = Unit
