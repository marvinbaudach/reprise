package de.reprise.spike

import android.graphics.Bitmap
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.core.graphics.get
import de.reprise.spike.ui.theme.oilFilmColour
import de.reprise.spike.ui.theme.toComposeColor

/**
 * The six colours one oil film is painted from, and the brushes that carry them.
 *
 * Six clouds are drawn per frame and every one of them wants a radial gradient.
 * Building those gradients in the draw would box six colour-stop arrays every
 * frame, on the path that exists to be cheap — so the brushes are built here,
 * once per artwork, in the unit space the draw transforms into place. Nothing
 * in them depends on the surface, the level or the clock, which is exactly why
 * they can outlive the frame.
 */
internal class OilFilmPalette(val clouds: List<Color>) {
    val brushes: List<Brush> = clouds.map(::cloudBrush)

    /**
     * How bright the film will read, before a surface or a scrim is involved.
     *
     * Six clouds screen-blend, so a palette lifted off a near-white cover carries far
     * more light onto the surface than one off a near-black cover does at the same
     * alpha. A caller that has no scrim of its own to absorb that difference reads this
     * and spends less alpha on the bright end.
     */
    val meanLuminance: Float = clouds.map(::luminance).average().toFloat()

    /**
     * Reads as [other] once [fraction] reaches 1 — the cross-fade between two
     * palettes.
     *
     * Both ends return the palette itself rather than a copy of it, so the two
     * states this spends almost all of its time in — cover, or spectrum — cost
     * nothing at all. Only the cross-fade between them builds anything, and it
     * lasts as long as a cross-fade does.
     */
    fun blendedTo(other: OilFilmPalette, fraction: Float): OilFilmPalette {
        val amount = fraction.coerceIn(0f, 1f)
        if (amount <= 0f) return this
        if (amount >= 1f) return other
        return OilFilmPalette(
            clouds.mapIndexed { index, colour ->
                mixColour(colour, other.clouds[index], amount)
            },
        )
    }
}

/**
 * The film's own falloff, standing in for a blur this app cannot afford.
 *
 * The design filters the whole cloud layer through `blur(34px)`, and Android
 * cannot: `RenderEffect` is API 31 and this app ships to 26, a boundary the fog
 * suite guards by reading this source. A CPU blur per frame is not an answer
 * either — six gradients into an offscreen buffer and a box blur over it costs
 * milliseconds on the main thread, every frame, for a background light.
 *
 * So the blur is folded into the gradient instead. What a 34 px blur does to a
 * cloud whose gradient radius is barely three times that is not a cosmetic
 * softening: it flattens the core and drags a long tail out past the edge. The
 * stops below are that shape — an approximate Gaussian with no plateau at the
 * centre and no edge at all, carried to the full radius rather than stopping at
 * 70% the way the unblurred design does. Combined with [NowPlayingOilFilmSpec.spread] it is
 * what makes six clouds read as one film rather than as six discs.
 *
 * Every stop is the colour itself at a lower alpha rather than the transparent
 * constant, which is black: interpolating towards black would lay a grey ring
 * around every cloud, and that ring is precisely the hard edge the design
 * forbids.
 */
private fun cloudBrush(colour: Color): Brush = Brush.radialGradient(
    colorStops = arrayOf(
        0f to colour.copy(alpha = 1f),
        0.15f to colour.copy(alpha = 0.90f),
        0.30f to colour.copy(alpha = 0.68f),
        0.45f to colour.copy(alpha = 0.44f),
        0.60f to colour.copy(alpha = 0.24f),
        0.75f to colour.copy(alpha = 0.10f),
        0.90f to colour.copy(alpha = 0.03f),
        1f to colour.copy(alpha = 0f),
    ),
    center = androidx.compose.ui.geometry.Offset.Zero,
    radius = 1f,
)

/**
 * The film the spectrum stands in front of. Constant, so built once and kept.
 */
internal val VisualizerRampPalette: OilFilmPalette by lazy {
    spreadOilFilmClouds(visualizerRampQuadrants())
}

/**
 * Four quadrant means of the artwork, lifted so a black cover still glows.
 *
 * The cover is read at 24x24 and averaged per quadrant rather than bucketed by
 * dominance the way [extractAmbientArtworkColors] reads it. The difference is
 * what the two are for: the ambient fields want the colours a listener would
 * name, the film wants four colours that sit in different corners of the
 * picture, because that is what makes it iridescent rather than tinted.
 *
 * The lift is the part that earns its place. A death-metal cover is near black,
 * its quadrant means land under 20, and six clouds of near black screen-blend
 * into nothing at all. Raising each channel to `40 + v * 0.95` keeps the hue —
 * every channel moves by the same offset and the same factor — while giving the
 * darkest cover a floor to be visible from. The 210 ceiling is the other end:
 * a white cover would otherwise wash the film out to a flat bright field.
 */
internal fun extractOilFilmQuadrants(bitmap: Bitmap?): List<Color> {
    val source = bitmap?.takeIf { it.width > 0 && it.height > 0 } ?: return neutralQuadrants()
    val scaled = IntArray(SAMPLE_SIZE * SAMPLE_SIZE)
    for (y in 0 until SAMPLE_SIZE) {
        val sourceY = (y * source.height / SAMPLE_SIZE).coerceAtMost(source.height - 1)
        for (x in 0 until SAMPLE_SIZE) {
            val sourceX = (x * source.width / SAMPLE_SIZE).coerceAtMost(source.width - 1)
            scaled[y * SAMPLE_SIZE + x] = source[sourceX, sourceY]
        }
    }
    val half = SAMPLE_SIZE / 2
    return listOf(0 to 0, 1 to 0, 0 to 1, 1 to 1).map { (quadrantX, quadrantY) ->
        var red = 0L
        var green = 0L
        var blue = 0L
        var count = 0
        for (y in quadrantY * half until (quadrantY + 1) * half) {
            for (x in quadrantX * half until (quadrantX + 1) * half) {
                val pixel = scaled[y * SAMPLE_SIZE + x]
                red += pixel ushr 16 and 0xff
                green += pixel ushr 8 and 0xff
                blue += pixel and 0xff
                count += 1
            }
        }
        argb(lift(red, count), lift(green, count), lift(blue, count)).toComposeColor()
    }
}

private fun lift(total: Long, count: Int): Int {
    if (count == 0) return DARK_FLOOR
    val mean = total.toFloat() / count
    return (DARK_FLOOR + mean * DARK_GAIN).toInt().coerceIn(0, LIGHT_CEILING)
}

private fun neutralQuadrants(): List<Color> = List(QUADRANT_COUNT) {
    argb(DARK_FLOOR, DARK_FLOOR, DARK_FLOOR).toComposeColor()
}

/** Packs three channels the way the theme's converter expects to read them. */
private fun argb(red: Int, green: Int, blue: Int): Int =
    (0xff shl 24) or (red shl 16) or (green shl 8) or blue

/**
 * The visualizer's own ramp, dimmed to a haze.
 *
 * Behind the spectrum there is no artwork to read — the picture *is* the
 * visualizer — so the film borrows the ramp the bars are drawn from:
 * `hsl(188 -> 315, 88%, 60%)`, cyan through to magenta, the same two ends
 * `reprise-core`'s bars mode paints. Sampled at four even steps and pulled 45%
 * of the way to the scene's near-black, which is what keeps it a light behind
 * the bars instead of a second, brighter copy of them.
 */
internal fun visualizerRampQuadrants(): List<Color> = List(QUADRANT_COUNT) { index ->
    val across = index.toFloat() / (QUADRANT_COUNT - 1)
    val hue = RAMP_HUE_START + (RAMP_HUE_END - RAMP_HUE_START) * across
    mixColour(hslColour(hue, RAMP_SATURATION, RAMP_LIGHTNESS), RampGround, RAMP_DARKEN)
}

/**
 * Six clouds out of four colours, then the film's own grade.
 *
 * Four corners of a cover give four colours, and four clouds read as four
 * clouds. The two extra are mixtures across the diagonals — first with fourth,
 * second with third — so that wherever two neighbours overlap there is already
 * a colour that belongs to both, and the overlap reads as one film folding into
 * itself rather than as two circles crossing.
 *
 * The grade is the second half of the design's filter stack. `saturate(1.6)`
 * and `contrast(1.2)` are applied to the colours here rather than to the
 * composited layer, for the same reason the blur is not applied at all: a
 * layer-wide colour filter is `RenderEffect`, and this app cannot have one.
 * Applied to the source colours it is not the identical arithmetic, but it is
 * the same intent, and unlike a per-frame filter it costs nothing per frame.
 */
internal fun spreadOilFilmClouds(quadrants: List<Color>): OilFilmPalette {
    require(quadrants.size == QUADRANT_COUNT) { "an oil film is spread from four colours" }
    val six = listOf(
        quadrants[0],
        quadrants[1],
        quadrants[2],
        quadrants[3],
        mixColour(quadrants[0], quadrants[3], 0.5f),
        mixColour(quadrants[1], quadrants[2], 0.5f),
    ).map(::saturated)
    val pivot = six.map(::luminance).average().toFloat()
    return OilFilmPalette(six.map { colour -> contrasted(colour, pivot) })
}

/**
 * `saturate(1.6)`, which is a per-colour operation and behaves as one.
 */
private fun saturated(colour: Color): Color {
    val luminance = luminance(colour)
    fun channel(value: Float): Float =
        (luminance + (value - luminance) * SATURATE).coerceIn(0f, 1f)
    return oilFilmColour(
        channel(colour.red),
        channel(colour.green),
        channel(colour.blue),
        colour.alpha,
    )
}

/**
 * `contrast(1.2)`, pivoted on the palette rather than on mid-grey.
 *
 * This is the one place the grade cannot simply be moved from the layer onto
 * the colours. CSS contrast pivots on 0.5 because it is applied to a composited
 * film that is already bright — six clouds screen-blended over one another sit
 * well above mid-grey, and pushing them away from 0.5 pushes them up. The
 * source colours do not sit there. A black cover's quadrants are lifted to
 * around 60 by design, and a 0.5 pivot would take that straight back down to
 * 49: the grade would spend its effort undoing the lift, and the covers the
 * lift exists for would be the ones it hurt.
 *
 * Pivoting on the palette's own mean luminance keeps what layer contrast is
 * actually for — the film's light regions and its dark ones move further apart
 * — without deciding in advance where the film as a whole ought to sit. A dark
 * cover comes out more differentiated rather than darker.
 */
private fun contrasted(colour: Color, pivot: Float): Color {
    fun channel(value: Float): Float = ((value - pivot) * CONTRAST + pivot).coerceIn(0f, 1f)
    return oilFilmColour(
        channel(colour.red),
        channel(colour.green),
        channel(colour.blue),
        colour.alpha,
    )
}

private fun luminance(colour: Color): Float =
    LUMA_RED * colour.red + LUMA_GREEN * colour.green + LUMA_BLUE * colour.blue

internal fun mixColour(from: Color, to: Color, amount: Float): Color {
    val part = amount.coerceIn(0f, 1f)
    return oilFilmColour(
        red = from.red + (to.red - from.red) * part,
        green = from.green + (to.green - from.green) * part,
        blue = from.blue + (to.blue - from.blue) * part,
        alpha = from.alpha + (to.alpha - from.alpha) * part,
    )
}

/** The `hsl()` the visualizer ramp is written in, so both ends read the same here. */
private fun hslColour(hue: Float, saturation: Float, lightness: Float): Color {
    val chroma = (1f - kotlin.math.abs(2f * lightness - 1f)) * saturation
    val sector = ((hue % 360f) + 360f) % 360f / 60f
    val second = chroma * (1f - kotlin.math.abs(sector % 2f - 1f))
    val (red, green, blue) = when (sector.toInt()) {
        0 -> Triple(chroma, second, 0f)
        1 -> Triple(second, chroma, 0f)
        2 -> Triple(0f, chroma, second)
        3 -> Triple(0f, second, chroma)
        4 -> Triple(second, 0f, chroma)
        else -> Triple(chroma, 0f, second)
    }
    val match = lightness - chroma / 2f
    return oilFilmColour(red + match, green + match, blue + match)
}

private const val QUADRANT_COUNT = 4
private const val SAMPLE_SIZE = 24
private const val DARK_FLOOR = 40
private const val DARK_GAIN = 0.95f
private const val LIGHT_CEILING = 210

private const val RAMP_HUE_START = 188f
private const val RAMP_HUE_END = 315f
private const val RAMP_SATURATION = 0.88f
private const val RAMP_LIGHTNESS = 0.60f
private const val RAMP_DARKEN = 0.45f
private val RampGround = argb(0x10, 0x10, 0x18).toComposeColor()

private const val SATURATE = 1.6f
private const val CONTRAST = 1.2f
private const val LUMA_RED = 0.213f
private const val LUMA_GREEN = 0.715f
private const val LUMA_BLUE = 0.072f
