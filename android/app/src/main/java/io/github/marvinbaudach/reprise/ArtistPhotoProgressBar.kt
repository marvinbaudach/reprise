package io.github.marvinbaudach.reprise

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.progressSemantics
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.clipPath
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay

internal enum class ArtistPhotoProgressPhase {
    PREPARING,
    RUNNING,
    PAUSED,
    COMPLETE,
}

internal data class ArtistPhotoProgress(
    val runId: Long,
    val phase: ArtistPhotoProgressPhase,
    val done: Long,
    val failed: Long,
    val total: Long,
)

private const val SUCCESS_DISMISS_DELAY_MS = 4_000L
private const val FAILURE_DISMISS_DELAY_MS = 10_000L

private class ArtistPhotoProgressHolder(var value: ArtistPhotoProgress? = null)

@Composable
internal fun ArtistPhotoEdgeProgress(
    progress: ArtistPhotoProgress?,
    dismiss: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val lastProgress = remember { ArtistPhotoProgressHolder() }
    if (progress != null) lastProgress.value = progress
    val shownProgress = progress ?: lastProgress.value
    LaunchedEffect(progress?.runId, progress?.phase, progress?.failed) {
        if (progress?.phase == ArtistPhotoProgressPhase.COMPLETE) {
            delay(
                if (progress.failed == 0L) SUCCESS_DISMISS_DELAY_MS else FAILURE_DISMISS_DELAY_MS,
            )
            dismiss()
        }
    }
    AnimatedVisibility(
        visible = progress != null,
        modifier = modifier,
        enter = fadeIn(tween(DELETION_LINE_FADE_MS)),
        exit = fadeOut(tween(DELETION_LINE_FADE_MS)),
    ) {
        shownProgress?.let { ArtistPhotoTrack(it) }
    }
}

@Composable
private fun ArtistPhotoTrack(progress: ArtistPhotoProgress) {
    if (
        progress.phase == ArtistPhotoProgressPhase.PREPARING ||
        progress.phase == ArtistPhotoProgressPhase.PAUSED
    ) {
        LinearProgressIndicator(
            color = MaterialTheme.colorScheme.primary,
            trackColor = MaterialTheme.colorScheme.outlineVariant,
            modifier = Modifier
                .fillMaxWidth()
                .height(3.dp)
                .progressSemantics()
                .testTag("artist-photo-progress-track"),
        )
        return
    }

    val total = progress.total.coerceAtLeast(1L).toFloat()
    val doneTarget = (progress.done.toFloat() / total).coerceIn(0f, 1f)
    val completedTarget = ((progress.done + progress.failed).toFloat() / total).coerceIn(0f, 1f)
    val done by animateFloatAsState(doneTarget, label = "artist photo downloads")
    val completed by animateFloatAsState(completedTarget, label = "artist photo requests")
    val trackColor = MaterialTheme.colorScheme.outlineVariant
    val doneColor = MaterialTheme.colorScheme.primary
    val failedColor = MaterialTheme.colorScheme.tertiary
    val description = "Artwork, ${progress.done} of ${progress.total} downloaded"
    Canvas(
        modifier = Modifier
            .fillMaxWidth()
            .height(3.dp)
            .progressSemantics(completedTarget)
            .semantics { stateDescription = description }
            .testTag("artist-photo-progress-track"),
    ) {
        val radius = size.height / 2f
        drawRoundRect(trackColor, cornerRadius = CornerRadius(radius, radius))
        val clip = Path().apply {
            addRoundRect(
                androidx.compose.ui.geometry.RoundRect(
                    rect = androidx.compose.ui.geometry.Rect(Offset.Zero, size),
                    cornerRadius = CornerRadius(radius, radius),
                ),
            )
        }
        clipPath(clip) {
            val drawnCompleted = completed.coerceIn(0f, 1f)
            val drawnDone = clampedArtistPhotoDoneFraction(done, drawnCompleted)
            drawRect(doneColor, size = Size(size.width * drawnDone, size.height))
            drawRect(
                color = failedColor,
                topLeft = Offset(size.width * drawnDone, 0f),
                size = Size(size.width * (drawnCompleted - drawnDone), size.height),
            )
        }
    }
}

internal fun clampedArtistPhotoDoneFraction(
    animatedDone: Float,
    animatedCompleted: Float,
): Float = animatedDone.coerceIn(0f, animatedCompleted.coerceIn(0f, 1f))
