package de.reprise.spike

import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.BlurredEdgeTreatment
import androidx.compose.ui.draw.blur
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import de.reprise.spike.ui.theme.AmbientTrueBlack
import de.reprise.spike.ui.theme.ambientFieldColors

/** The activity-owned truth for whether any ambient frame may be scheduled. */
internal class AmbientMotionController(
    private val observeScheduling: (Boolean) -> Unit = {},
) {
    private var attachedSurfaces = 0
    private var resumed = false
    private var screenInteractive = false
    private var systemAnimationsEnabled = false
    private var lastReported: Boolean? = null

    var scheduled by mutableStateOf(false)
        private set

    fun attach() {
        attachedSurfaces += 1
        update()
    }

    fun detach() {
        attachedSurfaces = (attachedSurfaces - 1).coerceAtLeast(0)
        update()
    }

    fun runtimeChanged(resumed: Boolean, screenInteractive: Boolean, animationsEnabled: Boolean) {
        this.resumed = resumed
        this.screenInteractive = screenInteractive
        systemAnimationsEnabled = animationsEnabled
        update()
    }

    fun stop() {
        resumed = false
        screenInteractive = false
        update()
    }

    private fun update() {
        val next = attachedSurfaces > 0 && resumed && screenInteractive && systemAnimationsEnabled
        scheduled = next
        if (lastReported != next) {
            lastReported = next
            observeScheduling(next)
        }
    }
}

internal val LocalAmbientMotionController = staticCompositionLocalOf { AmbientMotionController() }

@Composable
internal fun AmbientFields(
    artworkColors: AmbientArtworkColors?,
    modifier: Modifier = Modifier,
) {
    val controller = LocalAmbientMotionController.current
    DisposableEffect(controller) {
        controller.attach()
        onDispose { controller.detach() }
    }
    val colors = ambientFieldColors(artworkColors)
    Box(
        modifier = modifier
            .fillMaxSize()
            .background(AmbientTrueBlack)
            .testTag("ambient-fields"),
    ) {
        if (controller.scheduled) {
            DriftingFields(colors)
        } else {
            StaticFields(colors)
        }
    }
}

@Composable
private fun BoxScope.DriftingFields(colors: List<Color>) {
    val transition = rememberInfiniteTransition(label = "ambient-drift")
    val first by transition.animateFloat(
        initialValue = -1f,
        targetValue = 1f,
        animationSpec = ambientPeriod(13_000),
        label = "ambient-field-13s",
    )
    val second by transition.animateFloat(
        initialValue = 1f,
        targetValue = -1f,
        animationSpec = ambientPeriod(19_000),
        label = "ambient-field-19s",
    )
    val third by transition.animateFloat(
        initialValue = -0.4f,
        targetValue = 0.8f,
        animationSpec = ambientPeriod(24_000),
        label = "ambient-field-24s",
    )
    AmbientField(colors[0], Alignment.TopStart, first, 0.35f)
    AmbientField(colors[1], Alignment.CenterEnd, second, -0.55f)
    AmbientField(colors[2], Alignment.BottomCenter, third, 0.7f)
}

private fun ambientPeriod(periodMillis: Int) = infiniteRepeatable<Float>(
    animation = tween(durationMillis = periodMillis / 2),
    repeatMode = RepeatMode.Reverse,
)

@Composable
private fun BoxScope.StaticFields(colors: List<Color>) {
    AmbientField(colors[0], Alignment.TopStart, -0.55f, 0.35f)
    AmbientField(colors[1], Alignment.CenterEnd, 0.6f, -0.55f)
    AmbientField(colors[2], Alignment.BottomCenter, 0.15f, 0.7f)
}

@Composable
private fun BoxScope.AmbientField(
    color: Color,
    alignment: Alignment,
    driftX: Float,
    driftY: Float,
) {
    Box(
        modifier = Modifier
            .fillMaxSize(0.62f)
            .align(alignment)
            .graphicsLayer {
                translationX = driftX * 56.dp.toPx()
                translationY = driftY * 48.dp.toPx()
            }
            .blur(30.dp, BlurredEdgeTreatment.Unbounded)
            .background(color.copy(alpha = 0.62f), CircleShape),
    )
}
