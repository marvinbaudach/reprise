package de.reprise.spike

import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.withTransform
import kotlin.math.cos
import kotlin.math.sin

/** One cloud's placement in the field, as fractions of it. */
internal data class OilFilmBox(
    val left: Float,
    val top: Float,
    val width: Float,
    val height: Float,
)

/** One cloud's frame: where it has drifted, how it is turned, how far it is stretched. */
internal data class OilFilmCloud(
    val offsetXDp: Float,
    val offsetYDp: Float,
    val rotationDegrees: Float,
    val scaleX: Float,
    val scaleY: Float,
    val alpha: Float,
)

/**
 * Six soft clouds behind the cover, drifting on orbits that never line up.
 *
 * The layer this replaces answered the beat directly and flickered with it.
 * This one does not read the beat at all as far as *movement* goes: every
 * number below that decides where a cloud is comes from the wall clock, and the
 * music is allowed to touch two things only — how bright the film is, and a
 * tenth of its size. Position, rotation and colour are closed to it. That is
 * the whole reason a double bass no longer strobes the screen: there is no path
 * from a kick to a coordinate.
 *
 * The orbits are built so they never repeat. Each cloud's horizontal and
 * vertical frequencies rise on different, unrelated steps, its phase is a
 * multiple of 2.399 — a step around the circle that shares no fraction with a
 * turn — and each axis carries a second, faster harmonic at about a third of
 * the amplitude. Six clouds set up that way will drift past one another for as
 * long as the app is open without ever returning to a pose they held before.
 *
 * The clouds also do not stay round: they turn through ±45 degrees and stretch
 * unevenly in x and y on slower frequencies of their own. A circle that only
 * moves reads as a circle moving; one that is also being kneaded reads as a
 * film, which is the thing being drawn.
 */
internal object NowPlayingOilFilmSpec {
    const val cloudCount = 6

    /** How far past the surface the field reaches, so no cloud can show an edge. */
    const val overscan = 0.08f

    /** The global tempo of the drift. Two is a slow fold; the design tunes from here. */
    const val flow = 2f

    /**
     * How far past its box each cloud's tail is carried.
     *
     * The design's boxes are sized for clouds that will afterwards be blurred
     * by 34 px, and a blur that wide does not stay inside the thing it blurs —
     * it bleeds every cloud outward into its neighbours, which is what welds
     * the six of them into a film. Nothing here bleeds on its own, so the
     * clouds are grown until they overlap by about as much instead.
     */
    const val spread = 1.5f

    /**
     * The six boxes, taken from the design's own layout.
     *
     * Two of them are anchored to the right there and are written out as left
     * fractions here; a right anchor of `r` on a box of width `w` is a left
     * fraction of `1 - r - w`. Three of the six start outside the field on one
     * side or the other, which is deliberate: a cloud whose centre sits off the
     * edge contributes only its flank, and flanks are what keep the film from
     * looking like a row of circles.
     */
    private val boxes = listOf(
        OilFilmBox(left = -0.06f, top = 0.16f, width = 0.66f, height = 0.30f),
        OilFilmBox(left = 0.40f, top = 0.10f, width = 0.70f, height = 0.34f),
        OilFilmBox(left = 0.08f, top = 0.32f, width = 0.80f, height = 0.26f),
        OilFilmBox(left = 0.20f, top = 0.04f, width = 0.58f, height = 0.24f),
        OilFilmBox(left = -0.02f, top = 0.38f, width = 0.52f, height = 0.22f),
        OilFilmBox(left = 0.50f, top = 0.26f, width = 0.48f, height = 0.20f),
    )

    fun box(index: Int): OilFilmBox = boxes[index]

    /**
     * The pose cloud [index] holds [seconds] into the drift at this [level].
     *
     * Pure, and pure on purpose: it takes a clock and a number and returns six
     * floats, so the claim that the music cannot move a cloud is a claim a test
     * can make by calling this twice with two levels and comparing.
     */
    fun cloud(index: Int, seconds: Float, level: Float): OilFilmCloud {
        val phase = index * PHASE_STEP
        val omegaX = (BASE_OMEGA_X + index * OMEGA_X_STEP) * flow
        val omegaY = (BASE_OMEGA_Y + index * OMEGA_Y_STEP) * flow
        val bounded = level.coerceIn(0f, 1f)
        return OilFilmCloud(
            offsetXDp = sin(seconds * omegaX + phase) * (DRIFT_X + index * DRIFT_X_STEP) +
                sin(seconds * omegaX * HARMONIC_X + phase) * HARMONIC_X_AMPLITUDE,
            offsetYDp = cos(seconds * omegaY + phase * CROSS_PHASE) *
                (DRIFT_Y + index * DRIFT_Y_STEP) +
                cos(seconds * omegaY * HARMONIC_Y + phase) * HARMONIC_Y_AMPLITUDE,
            rotationDegrees = sin(seconds * (BASE_SPIN + index * SPIN_STEP) * flow + phase) * SPIN,
            scaleX = 1f + LEVEL_SCALE_X * bounded +
                KNEAD_X * sin(seconds * (BASE_KNEAD_X + index * KNEAD_X_STEP) * flow + phase),
            scaleY = 1f + LEVEL_SCALE_Y * bounded +
                KNEAD_Y * cos(seconds * (BASE_KNEAD_Y + index * KNEAD_Y_STEP) * flow + phase),
            alpha = (REST_ALPHA + ALPHA_SWING * bounded) * (1f - index * ALPHA_FALLOFF),
        )
    }

    /** A phase step that shares no fraction with a turn, so the orbits never re-align. */
    private const val PHASE_STEP = 2.399f
    private const val BASE_OMEGA_X = 0.15f
    private const val OMEGA_X_STEP = 0.041f
    private const val BASE_OMEGA_Y = 0.12f
    private const val OMEGA_Y_STEP = 0.033f
    private const val DRIFT_X = 40f
    private const val DRIFT_X_STEP = 7f
    private const val DRIFT_Y = 32f
    private const val DRIFT_Y_STEP = 6f
    private const val HARMONIC_X = 2.3f
    private const val HARMONIC_X_AMPLITUDE = 14f
    private const val HARMONIC_Y = 1.9f
    private const val HARMONIC_Y_AMPLITUDE = 11f
    private const val CROSS_PHASE = 1.3f
    private const val BASE_SPIN = 0.09f
    private const val SPIN_STEP = 0.024f
    private const val SPIN = 45f
    private const val BASE_KNEAD_X = 0.17f
    private const val KNEAD_X_STEP = 0.031f
    private const val KNEAD_X = 0.13f
    private const val BASE_KNEAD_Y = 0.14f
    private const val KNEAD_Y_STEP = 0.026f
    private const val KNEAD_Y = 0.15f

    /** All the music is allowed: a tenth of the size and a fifth of the brightness. */
    const val LEVEL_SCALE_X = 0.10f
    const val LEVEL_SCALE_Y = 0.08f
    const val REST_ALPHA = 0.34f
    const val ALPHA_SWING = 0.30f
    private const val ALPHA_FALLOFF = 0.05f
}

/**
 * Paints the film, and nothing else — the legibility scrims are drawn over it.
 *
 * Every cloud is one screen-blended draw of a gradient built once per artwork.
 * The brush lives in a unit circle at the origin and the matrix does the rest,
 * which is what lets the six of them be prepared ahead of the frame instead of
 * rebuilt inside it: nothing about a brush depends on the surface, the clock or
 * the level, and so nothing about it has to be recomputed when those change.
 */
internal fun DrawScope.drawNowPlayingOilFilm(
    palette: OilFilmPalette,
    horizontalShiftPx: Float,
    seconds: Float,
    level: Float,
    opacity: Float,
) {
    val boundedOpacity = opacity.coerceIn(0f, 1f)
    if (boundedOpacity <= 0f || size.width <= 0f || size.height <= 0f) return
    val fieldWidth = size.width * (1f + 2f * NowPlayingOilFilmSpec.overscan)
    val fieldHeight = size.height * (1f + 2f * NowPlayingOilFilmSpec.overscan)
    val fieldLeft = -NowPlayingOilFilmSpec.overscan * size.width + horizontalShiftPx
    val fieldTop = -NowPlayingOilFilmSpec.overscan * size.height
    repeat(NowPlayingOilFilmSpec.cloudCount) { index ->
        val cloud = NowPlayingOilFilmSpec.cloud(index, seconds, level)
        val alpha = cloud.alpha * boundedOpacity
        if (alpha <= 0f) return@repeat
        val box = NowPlayingOilFilmSpec.box(index)
        val width = fieldWidth * box.width * NowPlayingOilFilmSpec.spread
        val height = fieldHeight * box.height * NowPlayingOilFilmSpec.spread
        val centreX = fieldLeft + fieldWidth * (box.left + box.width / 2f) +
            cloud.offsetXDp * density
        val centreY = fieldTop + fieldHeight * (box.top + box.height / 2f) +
            cloud.offsetYDp * density
        withTransform({
            translate(centreX, centreY)
            rotate(cloud.rotationDegrees, Offset.Zero)
            scale(width / 2f * cloud.scaleX, height / 2f * cloud.scaleY, Offset.Zero)
        }) {
            drawCircle(
                brush = palette.brushes[index],
                radius = 1f,
                center = Offset.Zero,
                alpha = alpha,
                blendMode = BlendMode.Screen,
            )
        }
    }
}
