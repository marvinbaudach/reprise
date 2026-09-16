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
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.FrameRateCategory
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

private const val COVER_SIZE_DP = 272
private const val COVER_RADIUS_DP = 18f
private const val PLAYED_CENTRE_FRACTION = 0.34f
private const val TITLE_TO_ARTIST_GAP_DP = 6
private const val TITLE_PANEL_WIDTH_RATIO = 1.282f
private const val GLOW_TRANSLATION_FACTOR = 0.23f
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

internal data class NowPlayingPanelTransform(
    val translationX: Float,
    val scale: Float,
    val rotationDegrees: Float,
    val opacity: Float,
    val blurPx: Float,
    val saturation: Float,
) {
    val rotationForLayer: Float?
        get() = rotationDegrees.takeUnless { it.toRawBits() == 0f.toRawBits() }
}

internal fun nowPlayingPanelTransform(
    panelIndex: Int,
    positionPx: Float,
    widthPx: Float,
): NowPlayingPanelTransform {
    if (widthPx <= 0f) return NowPlayingPanelTransform(0f, 1f, 0f, 1f, 0f, 1f)
    if (positionPx == panelIndex * widthPx) {
        return NowPlayingPanelTransform(0f, 1f, 0f, 1f, 0f, 1f)
    }
    val fractionalIndex = positionPx / widthPx
    val delta = panelIndex - fractionalIndex
    val distance = min(1.6f, abs(delta))
    val near = max(0f, 1f - min(1f, abs(delta)))
    return NowPlayingPanelTransform(
        translationX = panelIndex * widthPx - positionPx,
        scale = 1f - distance * 0.13f,
        rotationDegrees = delta.coerceIn(-1f, 1f) * -3.5f,
        opacity = max(0f, 1f - distance * 0.75f),
        blurPx = (1f - near) * 5f,
        saturation = 0.4f + near * 0.6f,
    )
}

// The title rides its own panel offset at the wider ratio and nothing else.
// A half-panel-width term used to be subtracted here to re-centre a container
// that is laid out `TITLE_PANEL_WIDTH_RATIO` wide, but that container already
// centres its text on the screen, so the term only shifted every title
// 0.141 * width to the left -- 152 px on a 1080 px screen, enough to clip the
// first glyphs of a long title off the display. It also broke the symmetry the
// panels need: the neighbour on the left has to sit as far out as the one on
// the right, which only holds when this is odd in `positionPx`.
internal fun nowPlayingTitleTranslation(positionPx: Float): Float =
    -positionPx * TITLE_PANEL_WIDTH_RATIO

internal data class NowPlayingGlowTransform(
    val translationX: Float,
    val opacity: Float,
)

internal fun nowPlayingGlowTransform(
    panelIndex: Int,
    positionPx: Float,
    widthPx: Float,
): NowPlayingGlowTransform {
    if (widthPx <= 0f) return NowPlayingGlowTransform(0f, 1f)
    if (positionPx == panelIndex * widthPx) return NowPlayingGlowTransform(0f, 1f)
    val delta = panelIndex - positionPx / widthPx
    return NowPlayingGlowTransform(
        translationX = delta * widthPx * GLOW_TRANSLATION_FACTOR,
        opacity = max(0f, 1f - abs(delta) * 1.1f),
    )
}

internal data class NowPlayingProgressTransform(
    val translationY: Float,
    val opacity: Float,
    val scaleX: Float,
)

internal fun nowPlayingProgressTransform(
    currentIndex: Int,
    positionPx: Float,
    widthPx: Float,
): NowPlayingProgressTransform {
    val offset = if (widthPx > 0f) {
        min(1f, abs(positionPx / widthPx - currentIndex))
    } else {
        0f
    }
    return NowPlayingProgressTransform(
        translationY = -offset * 70f,
        opacity = 1f - offset * 0.9f,
        scaleX = 1f - offset * 0.06f,
    )
}

internal data class NowPlayingVisualBlend(
    val coverOpacity: Float,
    val barsOpacity: Float,
)

/**
 * Decides between a panel's cover and its bars from data availability alone.
 *
 * This used to be `near`, the panel's distance from the pager's centre —
 * which meant a neighbour with its spectrogram already loaded still showed
 * its cover, and the panel that had just become current could lose its bars
 * again mid-swipe, before it settled back to `near == 1`. Distance is still
 * used elsewhere (scale, blur, saturation): it earns a panel's depth, not
 * whether it is allowed to show what it already has.
 *
 * [dataAvailability] carries the animation, not this function: it is 0 or 1
 * at rest, and only spends time strictly between them while a caller fades
 * one into the other. `visualizerOpacity == 0` still forces the cover — a
 * listener who chose cover mode is not offering an opinion on data
 * availability.
 */
internal fun nowPlayingVisualBlend(
    visualizerOpacity: Float,
    dataAvailability: Float,
): NowPlayingVisualBlend {
    val availability = dataAvailability.coerceIn(0f, 1f)
    val bars = visualizerOpacity.coerceIn(0f, 1f) * availability
    return NowPlayingVisualBlend(coverOpacity = 1f - bars, barsOpacity = bars)
}

/**
 * Whether a panel has a real scene to draw as bars right now.
 *
 * A stored spectrogram always counts, and so does a scene the panel has
 * actually captured — never the mere fact of being live: a track the desktop
 * never analysed starts with nothing to scene, and this stays false until a
 * real, non-empty frame has been drawn. Getting this wrong opened the bars
 * slot the instant a panel became live, before its engine had anything to
 * show, which painted an empty (flat or black) scene over the cover during
 * the crossfade.
 *
 * The captured scene counts off the live slot too. The outgoing panel keeps
 * the bars it drew while live, and a neighbour that mirrored the live scene
 * during the swipe keeps those: in visualizer mode no panel falls back to its
 * cover for want of data, which is what used to flash the covers up mid-swipe.
 */
internal fun panelHasVisualData(
    storedFrameCount: Int,
    hasCapturedLiveScene: Boolean,
): Boolean = storedFrameCount > 0 || hasCapturedLiveScene

/**
 * Whether a neighbour draws the live panel's scene instead of its own.
 *
 * A neighbour without a stored spectrogram has no scene of its own — its
 * engine hears nothing — so while the swipe carries it onto the screen it
 * mirrors the live engine, tinted in its own accent. Off the screen
 * (`near == 0`) nothing is mirrored, so a resting neighbour costs no render;
 * a stored spectrogram is the panel's own picture and wins; the live panel is
 * the source, not a mirror.
 */
internal fun panelMirrorsLiveScene(
    isLivePanel: Boolean,
    storedFrameCount: Int,
    near: Float,
    liveSceneAvailable: Boolean,
): Boolean = !isLivePanel && storedFrameCount == 0 && near > 0f && liveSceneAvailable

/**
 * Whether a panel drawing the live scene still has to poll for its first one.
 *
 * `sceneBytes()` is the only place [FrozenSceneBytes] learns that real data
 * has landed, so a panel that owns or mirrors the live scene must keep
 * evaluating it even while [panelHasVisualData] is still false — otherwise it
 * could never leave that state. Only while bars were actually asked for, so a
 * panel viewed in pure cover mode never pays for a scene it will not draw.
 */
internal fun panelAwaitsFirstLiveScene(
    visualizerOpacity: Float,
    drawsLiveScene: Boolean,
    hasCapturedLiveScene: Boolean,
): Boolean = visualizerOpacity > 0f && drawsLiveScene && !hasCapturedLiveScene

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
    positionPx: Float = 0f,
    currentIndex: Int = 0,
    panels: List<PlayPanel> = listOf(PlayPanel(currentIndex, track)),
    visualizerOpacity: Float = 0f,
    cueRevision: Int = 0,
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
        val liveScene = remember { LiveSceneHandle() }
        Box(Modifier.fillMaxSize().testTag("now-playing-scene")) {
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
    )
    if (isLivePanel) {
        DisposableEffect(liveScene, visualEngine) {
            liveScene.engine = visualEngine
            onDispose { if (liveScene.engine === visualEngine) liveScene.engine = null }
        }
    }
    val frameSink = remember(visualEngine) { visualEngine?.let(::visualSceneFrameSink) }
    val drawRevision = DriveScene(frames, state, playback, motion, frameSink)
    val power = motion.sceneRenderPower()
    val transform = nowPlayingPanelTransform(panel.index, positionPx, widthPx)
    val glow = nowPlayingGlowTransform(panel.index, positionPx, widthPx)
    val distance = if (widthPx > 0f) abs(panel.index - positionPx / widthPx) else 0f
    val near = max(0f, 1f - min(1f, distance))
    val frozenScene = rememberFrozenSceneBytes(panel.track.id)
    val mirroredEngine = liveScene.engine.takeIf {
        panelMirrorsLiveScene(isLivePanel, frames.frameCount, near, liveSceneAvailable = it != null)
    }
    val hasVisualData = panelHasVisualData(frames.frameCount, frozenScene.hasCapturedScene)
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

    Canvas(
        modifier = Modifier
            .fillMaxSize()
            .graphicsLayer { translationX = glow.translationX },
    ) {
        observeSceneFrame(drawRevision)
        val center = Offset(size.width / 2f, size.height * PLAYED_CENTRE_FRACTION)
        drawPlayedNowPlayingFog(
            fog = fog,
            center = center,
            state = state,
            visualizerOpacity = barsOpacity,
            opacity = glow.opacity,
            rotationsEnabled = power.fogRotates,
        )
        drawPlayedNowPlayingShimmer(
            fog = fog,
            center = center,
            state = state,
            opacity = glow.opacity,
            rotationsEnabled = power.fogRotates,
        )
    }

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
): VisualSceneEngine? {
    val factory = visualSceneFactoryForPanel(live, LocalVisualSceneEngineFactory.current)
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
 * The live panel's engine, published for the neighbours to mirror.
 *
 * Owned by the scene, written by whichever panel is live, read by a neighbour
 * the swipe has carried onto the screen (see [panelMirrorsLiveScene]). It is
 * only ever the current engine or null; a neighbour never keeps it past the
 * frame it drew.
 */
internal class LiveSceneHandle {
    var engine: VisualSceneEngine? by mutableStateOf(null)
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
private fun observeSceneFrame(@Suppress("UNUSED_PARAMETER") revision: Int) = Unit
