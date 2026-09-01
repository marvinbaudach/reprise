package io.github.marvinbaudach.reprise

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalDensity
import kotlin.math.abs
import kotlin.math.min

internal fun topEdgeAccentScale(deviationPx: Float, widthPx: Float): Float =
    if (widthPx > 0f) {
        min(1f, abs(deviationPx) / (widthPx * TRACK_COMMIT_DISTANCE_FRACTION))
    } else {
        0f
    }

internal data class TopEdgeAccentTransform(
    val scaleX: Float,
    val transformOrigin: TransformOrigin,
)

internal fun topEdgeAccentTransform(
    deviationPx: Float,
    widthPx: Float,
): TopEdgeAccentTransform = TopEdgeAccentTransform(
    scaleX = topEdgeAccentScale(deviationPx, widthPx),
    transformOrigin = if (deviationPx > 0f) {
        TransformOrigin(1f, 0.5f)
    } else {
        TransformOrigin(0f, 0.5f)
    },
)

@Composable
internal fun TopEdgeAccentLine(
    deviationPx: Float,
    widthPx: Float,
    fingerDown: Boolean,
    animationsEnabled: Boolean,
) {
    val accent = MaterialTheme.colorScheme.primary
    val density = LocalDensity.current
    val transform = topEdgeAccentTransform(deviationPx, widthPx)
    Canvas(
        modifier = Modifier
            .fillMaxWidth()
            .height(with(density) { TOP_EDGE_LINE_HEIGHT_PX.toDp() })
            .graphicsLayer {
                scaleX = transform.scaleX
                transformOrigin = transform.transformOrigin
                alpha = if (fingerDown && animationsEnabled) 1f else 0f
            },
    ) {
        drawRect(
            brush = Brush.horizontalGradient(
                listOf(accent.copy(alpha = 0f), accent, accent.copy(alpha = 0f)),
            ),
        )
    }
}

private const val TOP_EDGE_LINE_HEIGHT_PX = 2f
