package de.reprise.spike

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.progressSemantics
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.clipPath
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
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
private const val VISIBILITY_ANIMATION_MS = 200

@Composable
internal fun ArtistPhotoProgressBar(
    progress: ArtistPhotoProgress?,
    dismiss: () -> Unit,
    inSettings: Boolean = false,
) {
    var displayedProgress by remember { mutableStateOf(progress) }
    LaunchedEffect(progress) {
        if (progress != null) displayedProgress = progress
    }
    LaunchedEffect(progress?.runId, progress?.phase, progress?.failed) {
        if (progress?.phase == ArtistPhotoProgressPhase.COMPLETE && progress.failed == 0L) {
            delay(SUCCESS_DISMISS_DELAY_MS)
            dismiss()
        }
    }
    AnimatedVisibility(
        visible = progress != null,
        enter = expandVertically(tween(VISIBILITY_ANIMATION_MS)) +
            fadeIn(tween(VISIBILITY_ANIMATION_MS)),
        exit = shrinkVertically(tween(VISIBILITY_ANIMATION_MS)) +
            fadeOut(tween(VISIBILITY_ANIMATION_MS)),
    ) {
        displayedProgress?.let { update ->
            ArtistPhotoProgressCard(update, dismiss, inSettings)
        }
    }
}

@Composable
private fun ArtistPhotoProgressCard(
    progress: ArtistPhotoProgress,
    dismiss: () -> Unit,
    inSettings: Boolean,
) {
    val outer = if (inSettings) {
        Modifier.padding(top = 12.dp)
    } else {
        Modifier.padding(horizontal = 12.dp).padding(bottom = 8.dp)
    }
    Surface(
        color = MaterialTheme.colorScheme.surfaceContainerHigh,
        shape = RoundedCornerShape(10.dp),
        modifier = outer
            .fillMaxWidth()
            .border(1.dp, MaterialTheme.colorScheme.outlineVariant, RoundedCornerShape(10.dp))
            .testTag("artist-photo-progress"),
    ) {
        Column(
            modifier = Modifier.padding(
                horizontal = 12.dp,
                vertical = if (inSettings) 12.dp else 11.dp,
            ),
        ) {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Text(
                    text = progress.label,
                    color = MaterialTheme.colorScheme.onSurface,
                    style = MaterialTheme.typography.labelMedium.copy(fontSize = 12.sp),
                    modifier = Modifier.weight(1f).testTag("artist-photo-progress-label"),
                )
                if (progress.phase != ArtistPhotoProgressPhase.PREPARING) {
                    Text(
                        text = "${progress.done} / ${progress.total}",
                        color = MaterialTheme.colorScheme.primary,
                        style = MaterialTheme.typography.labelMedium.copy(fontSize = 12.sp),
                        modifier = Modifier.testTag("artist-photo-progress-counter"),
                    )
                }
                IconButton(
                    onClick = dismiss,
                    modifier = Modifier
                        .size(48.dp)
                        .semantics { contentDescription = "Hide progress" },
                ) {
                    Text(
                        text = "×",
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        style = MaterialTheme.typography.titleMedium,
                    )
                }
            }
            Spacer(Modifier.height(if (inSettings) 9.dp else 8.dp))
            ArtistPhotoTrack(progress)
            if (progress.phase == ArtistPhotoProgressPhase.COMPLETE && progress.failed > 0L) {
                Spacer(Modifier.height(8.dp))
                Text(
                    text = "${progress.failed} without a photo",
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    style = MaterialTheme.typography.labelSmall.copy(fontSize = 11.sp),
                    modifier = Modifier.testTag("artist-photo-progress-failure"),
                )
            }
        }
    }
}

@Composable
private fun ArtistPhotoTrack(progress: ArtistPhotoProgress) {
    val shape = RoundedCornerShape(50)
    if (progress.phase == ArtistPhotoProgressPhase.PREPARING) {
        LinearProgressIndicator(
            color = MaterialTheme.colorScheme.primary,
            trackColor = MaterialTheme.colorScheme.outlineVariant,
            modifier = Modifier
                .fillMaxWidth()
                .height(4.dp)
                .clip(shape)
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
    val description = "Artist photos, ${progress.done} of ${progress.total} downloaded"
    Canvas(
        modifier = Modifier
            .fillMaxWidth()
            .height(4.dp)
            .progressSemantics(completedTarget)
            .semantics { contentDescription = description }
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
            drawRect(doneColor, size = Size(size.width * done, size.height))
            drawRect(
                color = failedColor,
                topLeft = Offset(size.width * done, 0f),
                size = Size(size.width * (completed - done).coerceAtLeast(0f), size.height),
            )
        }
    }
}

private val ArtistPhotoProgress.label: String
    get() = when (phase) {
        ArtistPhotoProgressPhase.PREPARING -> "Preparing artist photos"
        ArtistPhotoProgressPhase.RUNNING -> "Downloading artist photos"
        ArtistPhotoProgressPhase.PAUSED -> "Waiting for a connection"
        ArtistPhotoProgressPhase.COMPLETE -> "Artist photos complete"
    }
