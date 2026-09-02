package io.github.marvinbaudach.reprise

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.blur
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.material3.MaterialTheme

/** Draws geometry for a queue index whose track metadata has not arrived. */
@androidx.compose.runtime.Composable
internal fun EmptyNowPlayingPanelLayer(
    panelIndex: Int,
    positionPx: Float,
    widthPx: Float,
    coverTop: Dp,
) {
    val transform = nowPlayingPanelTransform(panelIndex, positionPx, widthPx)
    val plateColor = MaterialTheme.colorScheme.surfaceContainer
    val coverShadow = rememberCoverShadowBitmap()
    val density = LocalDensity.current
    val saturationFilter = cachedSaturationFilter(transform.saturation)

    Canvas(
        Modifier
            .offset(y = coverTop)
            .fillMaxWidth()
            .height(COVER_SIZE_DP.dp)
            .graphicsLayer {
                translationX = transform.translationX
                scaleX = transform.scale
                scaleY = transform.scale
                transform.rotationForLayer?.let { rotationZ = it }
                alpha = transform.opacity
                colorFilter = saturationFilter
            }
            .then(
                if (transform.blurPx.toRawBits() == 0f.toRawBits()) {
                    Modifier
                } else {
                    Modifier.blur(with(density) { transform.blurPx.toDp() })
                },
            ),
    ) {
        drawPlayedCover(
            artwork = null,
            center = Offset(size.width / 2f, size.height / 2f),
            fallback = plateColor,
            shadow = coverShadow,
            opacity = 1f,
        )
    }
}
