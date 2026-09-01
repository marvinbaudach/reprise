package io.github.marvinbaudach.reprise

import android.graphics.BlurMaskFilter
import android.graphics.Paint
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.graphics.drawscope.drawIntoCanvas
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import io.github.marvinbaudach.reprise.ui.theme.spectralColour
import kotlin.math.floor
import kotlin.math.min

private const val SEEK_TRACK_HEIGHT_DP = 32
private const val SEEK_TRACK_THICKNESS_DP = 3
private const val SPECTRAL_BAR_WIDTH_DP = 3
private const val SPECTRAL_CELL_WIDTH_DP = 5
private const val MAXIMUM_SPECTRAL_BAR_COUNT = 160
private const val MINIMUM_SPECTRAL_HEIGHT_DP = 2
private const val MAXIMUM_SPECTRAL_HEIGHT_DP = 26
private const val MINIMUM_AUDIBLE_HEIGHT_FRACTION = 0.15f
private const val PLAYED_ALPHA = 0.96f
private const val REMAINING_ALPHA = 0.34f
private const val SEEK_MARKER_WIDTH_PX = 3f
private const val SEEK_MARKER_GLOW_WIDTH_PX = 12f

/**
 * The real spectral seek track when Rust has bars, and the exact M10 plain
 * track when it does not. Compose only places and fades finished cells; their
 * height and RGB channels cross the FFI boundary already decided.
 */
@Composable
internal fun SpectralSeekTrack(
    trackId: Long,
    positionMs: Long,
    durationMs: Long,
    cueRevision: Int = 0,
    animationsEnabled: Boolean = true,
) {
    val analysis = LocalTrackAnalysis.current
    val revision = analysis.revision
    BoxWithConstraints(modifier = Modifier.fillMaxWidth()) {
        val count = floor(maxWidth / SPECTRAL_CELL_WIDTH_DP.dp).toInt()
            .coerceIn(1, MAXIMUM_SPECTRAL_BAR_COUNT)
        var bars by remember(trackId, count) { mutableStateOf<List<SpectralBar>?>(null) }
        val buildElapsed = remember(trackId) { Animatable(WAVEFORM_BUILD_MS.toFloat()) }
        val buildTrigger = remember { WaveformBuildTrigger() }
        val shouldBuild = remember(trackId, cueRevision, animationsEnabled) {
            buildTrigger.observe(cueRevision, animationsEnabled)
        }
        LaunchedEffect(analysis, trackId, count, revision) {
            analysis.loadBars(trackId, count) { answer -> bars = answer }
        }

        val ready = bars
        if (ready.isNullOrEmpty()) {
            LaunchedEffect(trackId) { buildElapsed.snapTo(WAVEFORM_BUILD_MS.toFloat()) }
            PlainSeekTrack(positionMs, durationMs)
        } else {
            val buildDuration = WAVEFORM_BUILD_MS +
                (ready.size - 1).coerceAtLeast(0) * WAVEFORM_STAGGER_MS
            LaunchedEffect(trackId, cueRevision, animationsEnabled) {
                if (!shouldBuild) {
                    buildElapsed.snapTo(buildDuration.toFloat())
                } else {
                    buildElapsed.snapTo(0f)
                    buildElapsed.animateTo(
                        buildDuration.toFloat(),
                        tween(buildDuration, easing = LinearEasing),
                    )
                }
            }
            SpectralBars(ready, positionMs, durationMs, buildElapsed.value)
        }
    }
}

@Composable
private fun SpectralBars(
    bars: List<SpectralBar>,
    positionMs: Long,
    durationMs: Long,
    buildElapsedMs: Float,
) {
    val seekMarkerPaint = rememberSeekMarkerPaint()
    val fraction = if (durationMs > 0) {
        (positionMs.toFloat() / durationMs.toFloat()).coerceIn(0f, 1f)
    } else {
        0f
    }
    val colours = bars.mapIndexed { index, bar ->
        val centreFraction = (index + 0.5f) / bars.size
        spectralColour(
            red = bar.red,
            green = bar.green,
            blue = bar.blue,
            alpha = if (centreFraction <= fraction) PLAYED_ALPHA else REMAINING_ALPHA,
        )
    }
    Canvas(
        modifier = Modifier
            .fillMaxWidth()
            .height(SEEK_TRACK_HEIGHT_DP.dp)
            .testTag("now-playing-seek-track"),
    ) {
        val stride = size.width / bars.size
        val barWidth = min(SPECTRAL_BAR_WIDTH_DP.dp.toPx(), stride * 0.72f)
        val maximumHeight = min(MAXIMUM_SPECTRAL_HEIGHT_DP.dp.toPx(), size.height)
        val minimumAudibleHeight = maximumHeight * MINIMUM_AUDIBLE_HEIGHT_FRACTION
        val silenceHeight = MINIMUM_SPECTRAL_HEIGHT_DP.dp.toPx()
        bars.forEachIndexed { index, bar ->
            val buildFraction = (
                (buildElapsedMs - index * WAVEFORM_STAGGER_MS) / WAVEFORM_BUILD_MS
                ).coerceIn(0f, 1f)
            val build = WAVEFORM_BUILD_EASING.transform(buildFraction)
            val buildScale = 0.1f + build * 0.9f
            val barHeight = (if (bar.silence) {
                silenceHeight
            } else {
                minimumAudibleHeight +
                    bar.level.coerceIn(0f, 1f) * (maximumHeight - minimumAudibleHeight)
            }) * buildScale
            val left = stride * (index + 0.5f) - barWidth / 2f
            drawRoundRect(
                color = colours[index].copy(alpha = colours[index].alpha * buildScale),
                topLeft = Offset(left, (size.height - barHeight) / 2f),
                size = Size(barWidth, barHeight),
                cornerRadius = CornerRadius(barWidth / 2f),
            )
        }
        drawSeekMarker(fraction, seekMarkerPaint)
    }
}

/** The M10 no-analysis track, kept pixel-for-pixel as the honest fallback. */
@Composable
internal fun PlainSeekTrack(positionMs: Long, durationMs: Long) {
    val elapsed = MaterialTheme.colorScheme.primary
    val remaining = MaterialTheme.colorScheme.outline
    val fraction = if (durationMs > 0) {
        (positionMs.toFloat() / durationMs.toFloat()).coerceIn(0f, 1f)
    } else {
        0f
    }
    val seekMarkerPaint = rememberSeekMarkerPaint()
    Canvas(
        modifier = Modifier
            .fillMaxWidth()
            .height(SEEK_TRACK_HEIGHT_DP.dp)
            .testTag("now-playing-seek-track"),
    ) {
        val centre = size.height / 2f
        val thickness = SEEK_TRACK_THICKNESS_DP.dp.toPx()
        val head = size.width * fraction
        drawLine(
            color = remaining,
            start = Offset(head, centre),
            end = Offset(size.width, centre),
            strokeWidth = thickness,
            cap = StrokeCap.Round,
        )
        if (head > 0f) {
            drawLine(
                color = elapsed,
                start = Offset(0f, centre),
                end = Offset(head, centre),
                strokeWidth = thickness,
                cap = StrokeCap.Round,
            )
        }
        drawSeekMarker(fraction, seekMarkerPaint)
    }
}

@Composable
private fun rememberSeekMarkerPaint(): Paint = remember {
    Paint().apply {
        color = NOW_PLAYING_ACCENT_200.toArgb()
        strokeWidth = SEEK_MARKER_WIDTH_PX
        maskFilter = BlurMaskFilter(SEEK_MARKER_GLOW_WIDTH_PX, BlurMaskFilter.Blur.NORMAL)
    }
}

private fun androidx.compose.ui.graphics.drawscope.DrawScope.drawSeekMarker(
    fraction: Float,
    glowPaint: Paint,
) {
    val x = size.width * fraction.coerceIn(0f, 1f)
    val top = Offset(x, 0f)
    val bottom = Offset(x, size.height)
    drawIntoCanvas { canvas ->
        canvas.nativeCanvas.drawLine(
            x,
            0f,
            x,
            size.height,
            glowPaint,
        )
    }
    drawLine(
        color = NOW_PLAYING_ACCENT_200,
        start = top,
        end = bottom,
        strokeWidth = SEEK_MARKER_WIDTH_PX,
    )
}

private const val WAVEFORM_BUILD_MS = 560
private const val WAVEFORM_STAGGER_MS = 5
private val WAVEFORM_BUILD_EASING = CubicBezierEasing(0.22f, 1f, 0.36f, 1f)
