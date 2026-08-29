package de.reprise.spike

import androidx.compose.animation.core.Animatable
import androidx.compose.foundation.Canvas
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.mutableDoubleStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset

/** Draws the motion-gated cover disc fixed behind an artist portrait. */
@Composable
internal fun ArtistPortraitShimmer(
    visual: ArtworkVisual?,
    playing: Boolean,
    coverDiameterDp: Float,
    centerFraction: Float,
    modifier: Modifier = Modifier,
) {
    val image = visual?.image ?: return
    val fog = rememberCoverFogBitmap(image, MaterialTheme.colorScheme.surface)
    val motion = LocalAmbientMotionController.current
    DisposableEffect(motion) {
        motion.attach()
        onDispose { motion.detach() }
    }
    val rotationsEnabled = motion.sceneRenderPower().fogRotates
    val elapsedSeconds = remember { mutableDoubleStateOf(0.0) }
    LaunchedEffect(rotationsEnabled) {
        if (!rotationsEnabled) return@LaunchedEffect
        var previousFrameNanos = withFrameNanos { it }
        while (true) {
            withFrameNanos { frameNanos ->
                elapsedSeconds.doubleValue +=
                    (frameNanos - previousFrameNanos).coerceAtLeast(0L) / NANOS_PER_SECOND
                previousFrameNanos = frameNanos
            }
        }
    }
    val swell = remember { Animatable(0f) }
    LaunchedEffect(playing) {
        swell.animateTo(if (playing) 1f else 0f)
    }
    Canvas(modifier) {
        drawNowPlayingShimmer(
            fog = fog,
            center = Offset(size.width / 2f, size.height * centerFraction.coerceIn(0f, 1f)),
            coverDiameterDp = coverDiameterDp,
            elapsedSeconds = elapsedSeconds.doubleValue,
            swell = swell.value,
            opacity = 1f,
            rotationsEnabled = rotationsEnabled,
        )
    }
}

private const val NANOS_PER_SECOND = 1_000_000_000.0
