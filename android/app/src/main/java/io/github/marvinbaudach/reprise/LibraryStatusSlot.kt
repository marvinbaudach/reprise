package io.github.marvinbaudach.reprise

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.SizeTransform
import androidx.compose.animation.core.MutableTransitionState
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.size
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp

private enum class ErrorKind {
    BROWSE,
    PLAYBACK,
    FAULT,
}

private data class StatusError(val kind: ErrorKind, val text: String)

private sealed interface StatusContent {
    data class Error(val value: StatusError) : StatusContent
    data object Deletion : StatusContent
}

@Composable
internal fun LibraryStatusSlot(
    browseError: String?,
    browseErrorOrigin: BrowseErrorOrigin?,
    surface: BrowseSurfaceGuard,
    dismissBrowseError: () -> Unit,
    surfaceState: MobileSurfaceViewModel,
    playback: LibraryPlayback,
    nowPlayingSheetState: MutableTransitionState<Boolean>,
    modifier: Modifier = Modifier,
) {
    var dismissedPlaybackError by remember { mutableStateOf<String?>(null) }
    var dismissedFault by remember { mutableStateOf<TransientMessage?>(null) }
    LaunchedEffect(playback.error) {
        if (playback.error != dismissedPlaybackError) dismissedPlaybackError = null
    }
    LaunchedEffect(playback.faultNotice) {
        if (playback.faultNotice != dismissedFault) dismissedFault = null
    }

    val currentBrowseError = browseError
        ?.takeIf { browseErrorOrigin?.let(surface::errorOriginIsCurrent) != false }
    val currentError = when {
        currentBrowseError != null -> StatusError(ErrorKind.BROWSE, currentBrowseError)
        playback.error != null && playback.error != dismissedPlaybackError -> {
            StatusError(ErrorKind.PLAYBACK, playback.error)
        }
        !surfaceState.dockMode &&
            !nowPlayingSheetState.currentState &&
            !nowPlayingSheetState.targetState &&
            playback.faultNotice != null &&
            playback.faultNotice != dismissedFault -> {
            StatusError(ErrorKind.FAULT, playback.faultNotice.text)
        }
        else -> null
    }
    val hasDeletion = surfaceState.deletionProgress != null || surfaceState.deletionMessage != null
    val content = currentError?.let(StatusContent::Error)
        ?: StatusContent.Deletion.takeIf { hasDeletion }

    AnimatedContent(
        targetState = content,
        modifier = modifier.fillMaxWidth(),
        contentKey = { target -> target?.let { it::class to (it as? StatusContent.Error)?.value?.kind } },
        transitionSpec = {
            fadeIn(tween(DELETION_LINE_FADE_MS)) togetherWith
                fadeOut(tween(DELETION_LINE_FADE_MS)) using SizeTransform(clip = false) { _, _ ->
                    tween(durationMillis = 0)
                }
        },
        label = "library status slot",
    ) { target ->
        when (target) {
            is StatusContent.Error -> LibraryErrorPill(target.value.text) {
                when (target.value.kind) {
                    ErrorKind.BROWSE -> dismissBrowseError()
                    ErrorKind.PLAYBACK -> dismissedPlaybackError = playback.error
                    ErrorKind.FAULT -> dismissedFault = playback.faultNotice
                }
            }
            StatusContent.Deletion -> DeletionMessageLine(surfaceState)
            null -> Unit
        }
    }
}

@Composable
private fun LibraryErrorPill(message: String, dismiss: () -> Unit) {
    Box(modifier = Modifier.fillMaxWidth(), contentAlignment = Alignment.TopCenter) {
        LibraryStatusPill(testTag = "library-status-error") {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    text = message,
                    color = MaterialTheme.colorScheme.error,
                    style = MaterialTheme.typography.bodyMedium,
                    textAlign = TextAlign.Center,
                    modifier = Modifier.weight(1f),
                )
                IconButton(
                    onClick = dismiss,
                    modifier = Modifier.size(32.dp).semantics { contentDescription = "Dismiss" },
                ) {
                    MaterialSymbol("close", "")
                }
            }
        }
    }
}
