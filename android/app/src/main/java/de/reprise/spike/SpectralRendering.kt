package de.reprise.spike

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color as ComposeColor
import androidx.compose.ui.unit.dp
import kotlin.math.floor
import uniffi.reprise_android_ffi.spectralBandColours

private const val BAR_WIDTH_DP = 3f
private const val BAR_GAP_DP = 2f
private const val MAX_BAR_COUNT = 160
private const val MINIMUM_LEVEL_FRACTION = 0.15f
private const val MAXIMUM_BAR_HEIGHT_DP = 26f
private const val SILENCE_DOT_HEIGHT_DP = 2f
private const val FALLBACK_HEIGHT_DP = 3f
private const val SPECTRUM_BAND_COUNT = 24
private const val SPECTRUM_BAR_WIDTH_FRACTION = 0.62f
private const val SPECTRUM_HEIGHT_FRACTION = 0.78f

internal data class SpectralBandColour(val red: Float, val green: Float, val blue: Float)

/** Rust owns this axis; the one boundary read is cached for the process. */
private object RustSpectralBandColours {
    val colours by lazy(LazyThreadSafetyMode.PUBLICATION) {
        spectralBandColours(SPECTRUM_BAND_COUNT.toUInt()).map { colour ->
            SpectralBandColour(
                red = colour.red.toFloat(),
                green = colour.green.toFloat(),
                blue = colour.blue.toFloat(),
            )
        }
    }
}

/** A system-boundary seam for host pixel tests; production always uses Rust. */
internal val LocalSpectralBandColours =
    staticCompositionLocalOf<List<SpectralBandColour>> { RustSpectralBandColours.colours }

/**
 * The shaped track picture used by both the seek track and the Preview mode.
 *
 * `null` is deliberately only a line: until analysis lands there is no audio
 * shape to imply. Present bars retain the Rust-provided height kind and colour.
 */
@Composable
internal fun SpectralTrackBand(
    trackId: Long,
    modifier: Modifier = Modifier,
) {
    val renderData = LocalTrackRenderData.current
    val fallbackColour = MaterialTheme.colorScheme.outline
    BoxWithConstraints(modifier = modifier.fillMaxWidth()) {
        val barCount = spectralBarCount(maxWidth.value)
        val revision = renderData.revision
        val bars = remember(renderData, revision, trackId, barCount) {
            renderData.bars(trackId, barCount)
        }
        Canvas(modifier = Modifier.matchParentSize()) {
            if (bars == null) {
                val thickness = FALLBACK_HEIGHT_DP.dp.toPx()
                drawRoundRect(
                    color = fallbackColour,
                    topLeft = Offset(0f, (size.height - thickness) / 2f),
                    size = Size(size.width, thickness),
                    cornerRadius = CornerRadius(thickness / 2f),
                )
                return@Canvas
            }
            if (bars.isEmpty()) return@Canvas

            val slotWidth = size.width / bars.size
            val barWidth = BAR_WIDTH_DP.dp.toPx().coerceAtMost(slotWidth.coerceAtLeast(1f))
            val maximumHeight = MAXIMUM_BAR_HEIGHT_DP.dp.toPx().coerceAtMost(size.height)
            val minimumHeight = maximumHeight * MINIMUM_LEVEL_FRACTION
            val silenceHeight = SILENCE_DOT_HEIGHT_DP.dp.toPx()
            bars.forEachIndexed { index, bar ->
                val height = if (bar.silence) {
                    silenceHeight
                } else {
                    minimumHeight +
                        bar.level.coerceIn(0f, 1f) * (maximumHeight - minimumHeight)
                }
                val left = index * slotWidth + (slotWidth - barWidth) / 2f
                val top = (size.height - height) / 2f
                drawRoundRect(
                    color = bar.composeColour(),
                    topLeft = Offset(left, top),
                    size = Size(barWidth, height),
                    cornerRadius = CornerRadius(barWidth / 2f, barWidth / 2f),
                )
            }
        }
    }
}

/** One shared per-track availability answer for every visualizer selector. */
@Composable
internal fun trackRenderDataAvailable(trackId: Long?): Boolean {
    val renderData = LocalTrackRenderData.current
    val revision = renderData.revision
    return remember(renderData, revision, trackId) {
        trackId != null && renderData.bars(trackId, 1) != null
    }
}

/** The stored 24-band column nearest the current playback position. */
@Composable
internal fun SpectralSpectrumColumn(
    trackId: Long,
    position: Float,
    modifier: Modifier = Modifier,
) {
    val renderData = LocalTrackRenderData.current
    val revision = renderData.revision
    val column = remember(renderData, revision, trackId, position) {
        renderData.spectrumColumn(trackId, position.coerceIn(0f, 1f))
    }
    val colours = LocalSpectralBandColours.current
    Canvas(modifier = modifier) {
        if (column.isNullOrEmpty()) return@Canvas
        val count = minOf(column.size, colours.size)
        if (count == 0) return@Canvas
        val slotWidth = size.width / count
        val barWidth = (slotWidth * SPECTRUM_BAR_WIDTH_FRACTION).coerceAtLeast(1f)
        val maximumHeight = size.height * SPECTRUM_HEIGHT_FRACTION
        for (index in 0 until count) {
            val height = column[index].coerceIn(0, 255) / 255f * maximumHeight
            if (height < 0.5f) continue
            val colour = colours[index]
            val left = index * slotWidth + (slotWidth - barWidth) / 2f
            drawRoundRect(
                color = ComposeColor(
                    red = colour.red.coerceIn(0f, 1f),
                    green = colour.green.coerceIn(0f, 1f),
                    blue = colour.blue.coerceIn(0f, 1f),
                    alpha = 1f,
                ),
                topLeft = Offset(left, size.height - height),
                size = Size(barWidth, height),
                cornerRadius = CornerRadius(barWidth / 2f, barWidth / 2f),
            )
        }
    }
}

private fun spectralBarCount(widthDp: Float): Int {
    if (!widthDp.isFinite() || widthDp <= 0f) return 1
    return floor(widthDp / (BAR_WIDTH_DP + BAR_GAP_DP)).toInt().coerceIn(1, MAX_BAR_COUNT)
}

private fun TrackRenderBar.composeColour() = ComposeColor(
    red = red.coerceIn(0f, 1f),
    green = green.coerceIn(0f, 1f),
    blue = blue.coerceIn(0f, 1f),
    alpha = 1f,
)
