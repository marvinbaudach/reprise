package io.github.marvinbaudach.reprise

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.snap
import androidx.compose.animation.core.tween
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.SemanticsPropertyKey
import androidx.compose.ui.semantics.SemanticsPropertyReceiver
import androidx.compose.ui.semantics.isTraversalGroup
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.traversalIndex
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch

internal data class ArtistHeadShimmerTransform(
    val translationY: Float,
    val alpha: Float,
)

internal fun artistHeadShimmerTransform(
    firstVisibleIndex: Int,
    offsetPx: Int,
    headHeightPx: Int,
): ArtistHeadShimmerTransform {
    val height = headHeightPx.coerceAtLeast(1)
    if (firstVisibleIndex > 0) {
        return ArtistHeadShimmerTransform(translationY = -height.toFloat(), alpha = 0f)
    }
    val offset = offsetPx.coerceAtLeast(0)
    return ArtistHeadShimmerTransform(
        translationY = if (offset == 0) 0f else -offset.toFloat(),
        alpha = (1f - offset.toFloat() / height).coerceIn(0f, 1f),
    )
}

internal val ArtistHeaderScrolledKey = SemanticsPropertyKey<Boolean>("ArtistHeaderScrolled")
private var SemanticsPropertyReceiver.artistHeaderScrolled by ArtistHeaderScrolledKey

/** The artist title stays over the list and becomes opaque once content passes beneath it. */
@Composable
internal fun ArtistDetailTitleRow(
    artist: LibraryArtist,
    closeArtist: () -> Unit,
    scrolled: Boolean,
    modifier: Modifier = Modifier,
    withMenu: Boolean = true,
    reportStatusInset: Boolean = true,
    below: @Composable ColumnScope.() -> Unit = {},
) {
    val contextMenu = rememberTrackContextMenuAnchorState()
    val containerColor = animateColorAsState(
        targetValue = if (scrolled) MaterialTheme.colorScheme.surfaceContainer else Color.Transparent,
        animationSpec = if (scrolled) snap() else tween(),
        label = "artist title container",
    )
    val tapShield = if (scrolled) {
        Modifier.pointerInput(Unit) { detectTapGestures(onTap = {}) }
    } else {
        Modifier
    }
    val statusModifier = if (reportStatusInset) {
        Modifier.reportLibraryStatusTopInset("artist-detail-header")
    } else {
        Modifier.testTag("artist-detail-header")
    }
    Column(
        modifier = modifier
            .fillMaxWidth()
            .drawBehind { drawRect(containerColor.value) }
            .then(tapShield)
            .semantics {
                artistHeaderScrolled = scrolled
                isTraversalGroup = true
                traversalIndex = -1f
            }
            .then(statusModifier),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Row(
                modifier = Modifier
                    .weight(1f)
                    .clickable(onClick = closeArtist)
                    .padding(horizontal = 16.dp, vertical = 8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                MaterialSymbol("arrow_back", "Back to artists")
                Text(artist.name, style = MaterialTheme.typography.titleLarge)
            }
            if (withMenu) {
                Box {
                    IconButton(
                        onClick = { contextMenu.expanded = true },
                        modifier = Modifier.testTag("artist-detail-overflow"),
                    ) {
                        MaterialSymbol("more_vert", "More actions")
                    }
                    TrackContextMenu(anchor = contextMenu, target = artistMenuTarget(artist))
                }
            }
        }
        TrackContextMenuMessage(contextMenu)
        below()
    }
}

/** The single list item that moves the artist's Play action and portrait together. */
@Composable
internal fun ArtistDetailHead(
    artist: LibraryArtist,
    visual: ArtworkVisual?,
    scope: CoroutineScope,
    resolving: Boolean,
    message: TransientMessage?,
    onResolvingChanged: (Boolean) -> Unit,
    onMessageChanged: (TransientMessage?) -> Unit,
    modifier: Modifier = Modifier,
    onPlayAreaSizeChanged: (Int) -> Unit = {},
) {
    val credits by LocalArtistPortraitCredits.current.collectAsState()
    Column(modifier = modifier.fillMaxWidth()) {
        Column(modifier = Modifier.onSizeChanged { onPlayAreaSizeChanged(it.height) }) {
            ArtistPlayButton(
                artist = artist,
                scope = scope,
                resolving = resolving,
                message = message,
                onResolvingChanged = onResolvingChanged,
                onMessageChanged = onMessageChanged,
            )
        }
        ArtistPortraitHeader(
            visual = visual,
            // A portrait that failed to decode leaves the generated cover; a
            // credit beneath that would describe a photo nobody is looking at.
            credit = credits[artist.name.trim()].takeIf { visual?.generated == false },
        )
    }
}

/** Reports the moving clearance without adding space to the overlaid title row. */
@Composable
internal fun ArtistDetailStatusInset(height: Dp, modifier: Modifier = Modifier) {
    Box(
        modifier = modifier
            .width(0.dp)
            .height(height)
            .reportLibraryStatusTopInset("artist-detail-status-inset"),
    )
}

/**
 * Plays every track the artist has, however few of them the page has loaded.
 * The ids come off the main thread; until they arrive the button is off, so a
 * second tap cannot queue the same question twice.
 */
@Composable
private fun ArtistPlayButton(
    artist: LibraryArtist,
    scope: CoroutineScope,
    resolving: Boolean,
    message: TransientMessage?,
    onResolvingChanged: (Boolean) -> Unit,
    onMessageChanged: (TransientMessage?) -> Unit,
) {
    val artistTrackIds = LocalArtistTrackIds.current
    val controls = LocalPlaybackControls.current
    ListPlayButton(
        description = "Play ${artist.name}",
        enabled = !resolving,
        modifier = Modifier.testTag("artist-detail-play"),
        onClick = {
            onResolvingChanged(true)
            scope.launch {
                try {
                    resolveOffMain { artistTrackIds(artist) }.fold(
                        onSuccess = { ids -> controls.playTrackIds(ids, 0) },
                        onFailure = { error ->
                            onMessageChanged(
                                TransientMessage(couldNotLoadTracks(error)).after(message),
                            )
                        },
                    )
                } finally {
                    onResolvingChanged(false)
                }
            }
        },
    )
    TransientMessageText(message) { onMessageChanged(null) }
}
