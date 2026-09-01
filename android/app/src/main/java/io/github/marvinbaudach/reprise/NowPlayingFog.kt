package io.github.marvinbaudach.reprise

import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.drawscope.DrawScope
import io.github.marvinbaudach.reprise.ui.theme.AmbientTrueBlack
import io.github.marvinbaudach.reprise.ui.theme.NowPlayingClear
import kotlin.math.max

internal object NowPlayingFogSpec {
    private const val SWELL_LOW = 0.05f
    private const val SWELL_HIGH = 0.70f

    /**
     * The level normalised to the range the measured music actually occupies,
     * kept here for the shimmer.
     *
     * The fog itself no longer uses it. It reads a slow envelope of its own now
     * (see [io.github.marvinbaudach.reprise.scene.OilFilmEnvelope]), while the disc over the
     * cover reads the rate-capped level this normalises — two different answers
     * to the same hazard, one per layer.
     */
    fun normalizedSwell(value: Float): Float = normalize(value, SWELL_LOW, SWELL_HIGH)

    private fun normalize(value: Float, low: Float, high: Float): Float =
        ((value - low) / (high - low)).coerceIn(0f, 1f)

    /**
     * The scrim that keeps the title readable, measured from the cover centre.
     *
     * The vertical scrim below protects the top bar and the transport, and the
     * title sits between them, on bare fog. Against a near-white cover that
     * left white type at 2.32:1. The band is flat across the rows themselves —
     * 150 dp to 252 dp under the centre, where a two-line title and the artist
     * line fall — and fades out beyond them so no edge is visible.
     */
    const val titleScrimFadeTopDp = 128f
    const val titleScrimSolidTopDp = 150f
    const val titleScrimSolidBottomDp = 252f
    const val titleScrimFadeBottomDp = 296f
    const val titleScrimAlpha = 0.58f
}

/**
 * The light behind the cover: six drifting clouds, then the scrims over them.
 *
 * What used to stand here were two blurred copies of the artwork, scaled and
 * counter-rotated, both of them brightening and swelling with the beat. On
 * anything with a fast kick that read as a flicker at the tempo of the track —
 * the layer was answering every impulse the detector found, sixteen a second
 * on a double bass.
 *
 * The film that replaced it is built the other way round. Its motion comes from
 * the clock and nothing else, and the music is let in through one slow envelope
 * that moves only the top fifth of the brightness and a tenth of the size. Over
 * a song section that is clearly visible; over a bar it is nothing at all,
 * which is the point.
 *
 * The scrims are unchanged and still drawn last. They are not part of the film
 * and must not drift with it: their whole job is to hold the title and the
 * transport at a fixed, readable ground whatever the clouds are doing.
 */
internal fun DrawScope.drawNowPlayingFog(
    palette: OilFilmPalette?,
    center: Offset,
    seconds: Float,
    level: Float,
    opacity: Float,
    driftEnabled: Boolean,
) {
    val boundedOpacity = opacity.coerceIn(0f, 1f)
    if (palette != null && boundedOpacity > 0f) {
        drawNowPlayingOilFilm(
            palette = palette,
            // The swipe carries the film with the cover, at the same reduced
            // rate the old layers were offset by, so the light stays behind the
            // artwork instead of sliding out from under it.
            horizontalShiftPx = center.x - size.width / 2f,
            // A power gate that has switched the drift off freezes the clock
            // rather than the film: the composition stays, the motion stops.
            seconds = if (driftEnabled) seconds else 0f,
            level = level,
            opacity = boundedOpacity,
        )
    }
    drawFogLegibility(center, boundedOpacity)
}

private fun DrawScope.drawFogLegibility(center: Offset, opacity: Float) {
    if (opacity <= 0f) return
    val legibility = fogLegibility(center, opacity)
    drawRect(brush = legibility.vignette)
    drawRect(brush = legibility.edges)
    legibility.titleScrim?.let { scrim -> drawRect(brush = scrim) }
}

/**
 * The legibility gradients, rebuilt only when one of their inputs changed.
 *
 * They are a pure function of the surface, the cover centre and the fade — no
 * scene state reaches them — so one key always describes one set of pixels.
 * Rebuilding them per frame boxed three colour-stop arrays every draw,
 * including the draws where nothing on screen had moved.
 */
private fun DrawScope.fogLegibility(center: Offset, opacity: Float): FogLegibility {
    val key = FogLegibilityKey(center, opacity, size, density)
    val remembered = fogLegibilityMemo
    if (remembered != null && remembered.key == key) return remembered
    val built = FogLegibility(
        key = key,
        vignette = Brush.radialGradient(
            colorStops = arrayOf(
                0f to NowPlayingClear,
                0.58f to AmbientTrueBlack.copy(alpha = 0.08f * opacity),
                1f to AmbientTrueBlack.copy(alpha = 0.88f * opacity),
            ),
            center = center,
            radius = max(size.width, size.height) * 0.78f,
        ),
        edges = Brush.verticalGradient(
            colorStops = arrayOf(
                0f to AmbientTrueBlack.copy(alpha = 0.72f * opacity),
                0.28f to NowPlayingClear,
                0.68f to NowPlayingClear,
                1f to AmbientTrueBlack.copy(alpha = 0.82f * opacity),
            ),
        ),
        titleScrim = titleScrimBrush(center, opacity),
    )
    fogLegibilityMemo = built
    return built
}

/**
 * Holds the title band down to a readable ground, whatever the cover does.
 *
 * Expressed as four stops of one surface-wide gradient rather than a rect of
 * its own, so the band moves with the cover on any screen height. Null on a
 * surface that leaves the band no room, where nothing is drawn at all.
 */
private fun DrawScope.titleScrimBrush(center: Offset, opacity: Float): Brush? {
    if (size.height <= 0f) return null
    val peak = AmbientTrueBlack.copy(alpha = NowPlayingFogSpec.titleScrimAlpha * opacity)
    val fadeTop = stopAt(center, NowPlayingFogSpec.titleScrimFadeTopDp)
    val solidTop = stopAt(center, NowPlayingFogSpec.titleScrimSolidTopDp)
    val solidBottom = stopAt(center, NowPlayingFogSpec.titleScrimSolidBottomDp)
    val fadeBottom = stopAt(center, NowPlayingFogSpec.titleScrimFadeBottomDp)
    if (solidBottom <= solidTop) return null
    return Brush.verticalGradient(
        colorStops = arrayOf(
            fadeTop to NowPlayingClear,
            solidTop to peak,
            solidBottom to peak,
            fadeBottom to NowPlayingClear,
        ),
    )
}

/** One draw's worth of legibility gradients, kept for the next identical draw. */
private class FogLegibility(
    val key: FogLegibilityKey,
    val vignette: Brush,
    val edges: Brush,
    val titleScrim: Brush?,
)

/** Everything the gradients are derived from, and nothing else. */
private data class FogLegibilityKey(
    val center: Offset,
    val opacity: Float,
    val size: Size,
    val density: Float,
)

/** Frames are drawn one at a time, so the last set is all that is worth keeping. */
private var fogLegibilityMemo: FogLegibility? = null

/** One gradient stop, as a fraction of the surface, offset from the cover. */
private fun DrawScope.stopAt(center: Offset, offsetDp: Float): Float =
    ((center.y + offsetDp * density) / size.height).coerceIn(0f, 1f)
