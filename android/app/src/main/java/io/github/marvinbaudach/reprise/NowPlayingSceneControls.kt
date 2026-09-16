// Seek progress and transport controls for the Now Playing scene.
package io.github.marvinbaudach.reprise

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.boundsInRoot
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import io.github.marvinbaudach.reprise.ui.theme.NowPlayingOnBackdrop
import uniffi.reprise_android_ffi.AndroidRepeatMode

@Composable
internal fun SceneProgress(
    track: LibraryTrack,
    playback: PlaybackUiState,
    surfaceState: MobileSurfaceViewModel,
    cueRevision: Int,
    animationsEnabled: Boolean,
    transform: NowPlayingProgressTransform,
    onSeekBounds: (Rect) -> Unit,
    modifier: Modifier,
) {
    var layoutBounds by remember { mutableStateOf(Rect.Zero) }
    var seekSize by remember { mutableStateOf(IntSize.Zero) }
    val horizontalPaddingPx = with(LocalDensity.current) { 24.dp.toPx() }
    val reportedSeekBounds = transformedSeekBounds(
        layoutBounds = layoutBounds,
        seekSize = seekSize,
        horizontalPaddingPx = horizontalPaddingPx,
        transform = transform,
    )
    SideEffect { onSeekBounds(reportedSeekBounds) }

    Box(
        modifier = modifier.onGloballyPositioned { layoutBounds = it.boundsInRoot() },
    ) {
        Box(
            modifier = Modifier
                .graphicsLayer {
                    translationY = transform.translationY
                    alpha = transform.opacity
                    scaleX = transform.scaleX
                }
                .padding(horizontal = 24.dp),
        ) {
            SpectralSeekSlider(
                track.id,
                playback,
                surfaceState,
                cueRevision = cueRevision,
                animationsEnabled = animationsEnabled,
                onSeekSize = { seekSize = it },
            )
        }
    }
}

internal fun transformedSeekBounds(
    layoutBounds: Rect,
    seekSize: IntSize,
    horizontalPaddingPx: Float,
    transform: NowPlayingProgressTransform,
): Rect {
    if (layoutBounds == Rect.Zero || seekSize == IntSize.Zero) return Rect.Zero
    val untransformedLeft = layoutBounds.left + horizontalPaddingPx
    val pivotX = layoutBounds.center.x
    return Rect(
        left = pivotX + (untransformedLeft - pivotX) * transform.scaleX,
        top = layoutBounds.top + transform.translationY,
        right = pivotX +
            (untransformedLeft + seekSize.width - pivotX) * transform.scaleX,
        bottom = layoutBounds.top + seekSize.height + transform.translationY,
    )
}

@Composable
internal fun SceneTransport(
    playback: PlaybackUiState,
    cueRevision: Int,
    animationsEnabled: Boolean,
    onPrevious: () -> Unit,
    onNext: () -> Unit,
    modifier: Modifier,
) {
    val controls = LocalPlaybackControls.current
    Row(
        modifier = modifier
            .fillMaxWidth()
            .testTag("now-playing-transport"),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        FlatSceneButton(
            symbol = "shuffle",
            description = if (playback.shuffled) "Turn shuffle off" else "Turn shuffle on",
            active = playback.shuffled,
            tag = "now-playing-shuffle",
            onClick = { controls.setShuffle(!playback.shuffled) },
        )
        FlatSceneButton("skip_previous", "Previous track", onClick = onPrevious)
        ScenePauseButton(playback, cueRevision, animationsEnabled, controls::togglePause)
        FlatSceneButton("skip_next", "Next track", onClick = onNext)
        FlatSceneButton(
            symbol = if (playback.repeat == AndroidRepeatMode.ONE) "repeat_one" else "repeat",
            description = "Repeat ${playback.repeat.name.lowercase()}",
            active = playback.repeat != AndroidRepeatMode.OFF,
            tag = "now-playing-repeat",
            onClick = { controls.setRepeat(cycleRepeatMode(playback.repeat)) },
        )
    }
}

@Composable
private fun FlatSceneButton(
    symbol: String,
    description: String,
    active: Boolean = false,
    tag: String? = null,
    onClick: () -> Unit,
) {
    IconButton(
        onClick = onClick,
        modifier = Modifier
            .size(48.dp)
            .then(if (tag == null) Modifier else Modifier.testTag(tag))
            .semantics { selected = active }
            .then(
                if (active) {
                    Modifier
                        .clip(MaterialTheme.shapes.large)
                        .background(MaterialTheme.colorScheme.secondaryContainer)
                } else {
                    Modifier
                },
            ),
    ) {
        MaterialSymbol(
            symbol,
            description,
            tint = if (active) {
                MaterialTheme.colorScheme.onSecondaryContainer
            } else {
                NowPlayingOnBackdrop
            },
        )
    }
}

@Composable
private fun ScenePauseButton(
    playback: PlaybackUiState,
    cueRevision: Int,
    animationsEnabled: Boolean,
    onClick: () -> Unit,
) {
    val shape = RoundedCornerShape(28.dp)
    Box(Modifier.size(80.dp), contentAlignment = Alignment.Center) {
        PlayButtonPulse(cueRevision, animationsEnabled)
        IconButton(
            onClick = onClick,
            modifier = Modifier
                .size(80.dp)
                .testTag("now-playing-play")
                .clip(shape)
                .background(MaterialTheme.colorScheme.primary),
        ) {
            MaterialSymbol(
                name = playback.playPauseSymbol,
                contentDescription = playback.playPauseLabel,
                tint = NowPlayingOnBackdrop,
                sizeSp = 40,
            )
        }
    }
}
