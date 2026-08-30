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
        // The film first, the disc over it — the played view's own order.
        //
        // Only the film itself is borrowed, not `drawNowPlayingFog`: that one
        // follows the clouds with the legibility scrims, a surface-wide vignette
        // and two edge gradients tuned to hold a title over a full-bleed cover.
        // Drawn here they would black out the album rows behind nothing.
        fog?.let { prepared ->
            drawNowPlayingOilFilm(
                palette = prepared.palette,
                horizontalShiftPx = 0f,
                seconds = if (rotationsEnabled) elapsedSeconds.doubleValue.toFloat() else 0f,
                level = swell.value,
                opacity = filmOpacity(prepared.palette.meanLuminance),
            )
        }
        drawNowPlayingShimmer(
            fog = fog,
            center = Offset(size.width / 2f, size.height * centerFraction.coerceIn(0f, 1f)),
            coverDiameterDp = coverDiameterDp,
            elapsedSeconds = elapsedSeconds.doubleValue,
            swell = swell.value,
            opacity = 1f,
            rotationsEnabled = rotationsEnabled,
            alphaScale = NowPlayingShimmerSpec.ON_BARE_SURFACE_SCALE,
        )
    }
}

private const val NANOS_PER_SECOND = 1_000_000_000.0

/**
 * How much of the played view's film the artist page keeps, per artwork.
 *
 * Two reductions, and they answer different things. The first is flat: the clouds are
 * read here without the scrims that sit over them in the played view, and at full
 * strength the film crowded the portrait rather than lighting it, so a tenth comes off. The second follows the palette, because the flat one alone still left a
 * near-white portrait pulling the eye away from the covers — six clouds lifted off a
 * bright artwork carry far more light at the same alpha than the same six off a dark
 * one. So the bright end spends about half of what the dark end does, and the film stays an
 * accent on both.
 */
private fun filmOpacity(meanLuminance: Float): Float =
    FILM_OPACITY * (1f - BRIGHT_FALLOFF * meanLuminance.coerceIn(0f, 1f))

private const val FILM_OPACITY = 0.9f
private const val BRIGHT_FALLOFF = 0.7f
