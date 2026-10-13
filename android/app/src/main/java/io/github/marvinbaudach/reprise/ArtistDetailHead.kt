package io.github.marvinbaudach.reprise

import androidx.compose.animation.animateColorAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.SemanticsPropertyKey
import androidx.compose.ui.semantics.SemanticsPropertyReceiver
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
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
    below: @Composable ColumnScope.() -> Unit = {},
) {
    val contextMenu = rememberTrackContextMenuAnchorState()
    val containerColor by animateColorAsState(
        targetValue = if (scrolled) MaterialTheme.colorScheme.surfaceContainer else Color.Transparent,
        label = "artist title container",
    )
    Column(
        modifier = modifier
            .fillMaxWidth()
            .background(containerColor)
            .semantics { artistHeaderScrolled = scrolled }
            .reportLibraryStatusTopInset("artist-detail-header"),
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
    modifier: Modifier = Modifier,
) {
    Column(modifier = modifier.fillMaxWidth()) {
        ArtistPlayButton(artist)
        ArtistPortraitHeader(visual)
    }
}

/**
 * Plays every track the artist has, however few of them the page has loaded.
 * The ids come off the main thread; until they arrive the button is off, so a
 * second tap cannot queue the same question twice.
 */
@Composable
private fun ArtistPlayButton(artist: LibraryArtist) = key(artist.name) {
    val artistTrackIds = LocalArtistTrackIds.current
    val controls = LocalPlaybackControls.current
    // The key ties the scope to the artist: a play still resolving when the
    // page moves to another artist is cancelled, as it is when the page closes.
    // It is the name, not the row, because the ids are looked up by name and a
    // refresh after a delete rebuilds the row with new counts mid-resolution.
    val scope = rememberCoroutineScope()
    var resolving by remember { mutableStateOf(false) }
    var message by remember { mutableStateOf<TransientMessage?>(null) }
    ListPlayButton(
        description = "Play ${artist.name}",
        enabled = !resolving,
        modifier = Modifier.testTag("artist-detail-play"),
        onClick = {
            resolving = true
            scope.launch {
                try {
                    resolveOffMain { artistTrackIds(artist) }.fold(
                        onSuccess = { ids -> controls.playTrackIds(ids, 0) },
                        onFailure = { error ->
                            message = TransientMessage(couldNotLoadTracks(error)).after(message)
                        },
                    )
                } finally {
                    resolving = false
                }
            }
        },
    )
    TransientMessageText(message) { message = null }
}
