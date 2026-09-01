package io.github.marvinbaudach.reprise

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.LinearOutSlowInEasing
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.requiredSize
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.unit.dp

@Composable
internal fun PlayButtonPulse(cueRevision: Int, animationsEnabled: Boolean) {
    val progress = remember { Animatable(1f) }
    val trigger = remember { ConfirmationCueTrigger() }
    val shouldAnimate = remember(cueRevision, animationsEnabled) {
        trigger.observe(cueRevision, animationsEnabled)
    }
    LaunchedEffect(cueRevision, animationsEnabled) {
        if (!shouldAnimate) {
            progress.snapTo(1f)
            return@LaunchedEffect
        }
        progress.snapTo(0f)
        progress.animateTo(1f, tween(PULSE_DURATION_MS, easing = LinearOutSlowInEasing))
    }
    val accent = MaterialTheme.colorScheme.primary
    Canvas(
        Modifier.requiredSize((PULSE_RADIUS_DP * PULSE_END_SCALE * 2f).dp),
    ) {
        val scale = PULSE_START_SCALE + progress.value * (PULSE_END_SCALE - PULSE_START_SCALE)
        drawCircle(
            color = accent.copy(alpha = (0.5f * (1f - progress.value)).coerceIn(0f, 0.5f)),
            radius = PULSE_RADIUS_DP.dp.toPx() * scale,
            style = Stroke(width = PULSE_STROKE_PX),
        )
    }
}

private const val PULSE_RADIUS_DP = 26f
private const val PULSE_STROKE_PX = 1f
private const val PULSE_START_SCALE = 0.9f
private const val PULSE_END_SCALE = 1.9f
private const val PULSE_DURATION_MS = 620
