package io.github.marvinbaudach.reprise

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.LinearOutSlowInEasing
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import io.github.marvinbaudach.reprise.ui.theme.toComposeColor

@Composable
internal fun TopEdgeSweep(cueRevision: Int, animationsEnabled: Boolean) {
    val progress = remember { Animatable(1f) }
    val trigger = remember { ConfirmationCueTrigger() }
    val shouldAnimate = remember(cueRevision, animationsEnabled) {
        trigger.observe(cueRevision, animationsEnabled)
    }
    var widthPx by remember { mutableFloatStateOf(0f) }
    LaunchedEffect(cueRevision, animationsEnabled) {
        if (!shouldAnimate) {
            progress.snapTo(1f)
            return@LaunchedEffect
        }
        progress.snapTo(0f)
        progress.animateTo(1f, tween(SWEEP_DURATION_MS, easing = LinearOutSlowInEasing))
    }
    val opacity = if (progress.value <= SWEEP_OPACITY_PEAK) {
        progress.value / SWEEP_OPACITY_PEAK
    } else {
        (1f - progress.value) / (1f - SWEEP_OPACITY_PEAK)
    }.coerceIn(0f, 1f)
    val density = LocalDensity.current
    Canvas(
        modifier = Modifier
            .fillMaxWidth()
            .height(with(density) { SWEEP_HEIGHT_PX.toDp() })
            .onSizeChanged { widthPx = it.width.toFloat() }
            .graphicsLayer {
                translationX = (-1f + progress.value * 2f) * widthPx
                alpha = opacity
            },
    ) {
        drawRect(
            Brush.horizontalGradient(
                listOf(
                    NOW_PLAYING_ACCENT_200.copy(alpha = 0f),
                    NOW_PLAYING_ACCENT_200,
                    NOW_PLAYING_ACCENT_200.copy(alpha = 0f),
                ),
            ),
        )
    }
}

internal val NOW_PLAYING_ACCENT_200 = 0xFFE7E5FE.toInt().toComposeColor()
private const val SWEEP_HEIGHT_PX = 2f
private const val SWEEP_DURATION_MS = 620
private const val SWEEP_OPACITY_PEAK = 0.25f
