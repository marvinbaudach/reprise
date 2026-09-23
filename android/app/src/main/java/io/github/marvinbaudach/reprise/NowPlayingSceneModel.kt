package io.github.marvinbaudach.reprise

import androidx.compose.ui.FrameRateCategory
import kotlin.math.abs
import kotlin.math.max
import kotlin.math.min

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
 * the bars it drew while live until the new engine speaks, then mirrors that
 * one on its way out; a neighbour that mirrored the live scene during the
 * swipe keeps that picture. In visualizer mode no panel falls back to its
 * cover for want of data, which is what used to flash the covers up mid-swipe.
 *
 * A panel that *could* mirror the live engine ([panelCanMirrorLiveScene])
 * counts too, even at rest and even before its own stored spectrogram has
 * finished loading and before [FrozenSceneBytes] has latched a frame of its
 * own: whether it is on screen yet is a render-cost question
 * ([panelMirrorsLiveScene]'s `near` gate), not a data-availability one. Gating
 * this on `near` instead used to leave `dataAvailability` resting at 0 for a
 * neighbour without its own spectrogram, so the first drag pixel that turned
 * `near` positive flipped the target to 1 and the crossfade tween flashed the
 * cover up for its own duration at the start of every swipe. This stays a
 * pure rule change — the live panel itself never mirrors
 * ([panelCanMirrorLiveScene] is false for it), so it keeps requiring a real
 * captured frame, exactly as before.
 */
internal fun panelHasVisualData(
    storedFrameCount: Int,
    hasCapturedLiveScene: Boolean,
    canMirrorLiveScene: Boolean,
): Boolean = storedFrameCount > 0 || hasCapturedLiveScene || canMirrorLiveScene

/**
 * Whether a neighbour *could* draw the live panel's scene instead of its own,
 * regardless of whether it is currently on screen.
 *
 * A neighbour without a stored spectrogram has no scene of its own — its
 * engine hears nothing — so once the swipe carries it onto the screen it
 * mirrors the live engine, tinted in its own accent. The panel that has just
 * lost the live slot is such a neighbour too: it slides out with the bars of
 * what is playing rather than a frozen picture of what was. A stored
 * spectrogram is the panel's own picture and wins; the live panel is the
 * source, not a mirror.
 */
internal fun panelCanMirrorLiveScene(
    isLivePanel: Boolean,
    storedFrameCount: Int,
    liveSceneAvailable: Boolean,
): Boolean = !isLivePanel && storedFrameCount == 0 && liveSceneAvailable

/**
 * Whether a neighbour is actually drawing the live panel's scene right now.
 *
 * Same eligibility as [panelCanMirrorLiveScene], plus `near > 0f`: off the
 * screen nothing is mirrored, so a resting neighbour costs no render. This
 * gate is a render-cost decision only — it must not gate [panelHasVisualData]
 * too, or a panel eligible to mirror once dragged onscreen would flash its
 * cover for the first frames of every swipe while `near` catches up.
 */
internal fun panelMirrorsLiveScene(
    isLivePanel: Boolean,
    storedFrameCount: Int,
    near: Float,
    liveSceneAvailable: Boolean,
): Boolean = panelCanMirrorLiveScene(isLivePanel, storedFrameCount, liveSceneAvailable) && near > 0f

/**
 * Whether a newly created live engine should adopt the outgoing live engine's
 * bar shape instead of starting from zero.
 *
 * Production gives each panel a new lease over one shared live engine (see
 * [visualSceneFactoryForPanel]). The explicit `noteTrackChanged()` call resets
 * that engine's CAVA history, which otherwise leaves a bare peak cap with no
 * bars underneath for one frame; the seed carries the displayed shape across
 * that reset. Only the panel taking over the live slot adopts anything — a
 * non-live panel's engine never scenes live audio, and a panel that keeps the
 * live slot across a recomposition has no `previous` to speak of (`created`
 * did not change). In production `previous !== created` is always true
 * because every `create()` returns a new lease; it only guards test doubles
 * that return the same engine instance.
 */
internal fun shouldAdoptLiveShape(
    live: Boolean,
    previous: VisualSceneEngine?,
    created: VisualSceneEngine,
): Boolean = live && previous != null && previous !== created

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

internal const val TITLE_PANEL_WIDTH_RATIO = 1.282f
