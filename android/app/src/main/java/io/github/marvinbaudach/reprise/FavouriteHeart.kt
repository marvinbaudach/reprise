package io.github.marvinbaudach.reprise

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.size
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.unit.dp

internal const val TRACK_HEART_TAG = "track-heart"
internal val LocalNowPlayingActionsEnabled = staticCompositionLocalOf { true }

@Composable
internal fun FavouriteHeartButton(
    track: LibraryTrack,
    surfaceState: MobileSurfaceViewModel,
    modifier: Modifier = Modifier,
    sizeDp: Int = 48,
    iconSizeSp: Int = 28,
    tag: String = TRACK_HEART_TAG,
    enabled: Boolean = true,
    onConfirmed: (Boolean) -> Unit = {},
) {
    val setFavourite = LocalPlaybackControls.current::setFavourite
    val favourite = surfaceState.ratingOf(track) == 5
    var failure by remember(track.id) { mutableStateOf<TransientMessage?>(null) }
    val description = if (favourite) "Remove from favourites" else "Add to favourites"
    Column {
        IconButton(
            enabled = enabled,
            onClick = {
                val target = !favourite
                setFavourite(track.id, target) { message ->
                    if (message == null) {
                        surfaceState.confirmFavourite(track.id, target)
                        onConfirmed(target)
                        failure = null
                    } else {
                        failure = TransientMessage(message).after(failure)
                    }
                }
            },
            modifier = modifier
                .size(sizeDp.dp)
                .testTag(tag)
                .semantics {
                    stateDescription = if (favourite) "Favourite" else "Not a favourite"
                },
        ) {
            FavouriteHeartIcon(
                favourite = favourite,
                contentDescription = description,
                tint = MaterialTheme.colorScheme.tertiary,
                sizeSp = iconSizeSp,
            )
        }
        TransientMessageText(failure) { failure = null }
    }
}

@Composable
internal fun FavouriteHeartIcon(
    favourite: Boolean,
    contentDescription: String,
    tint: Color,
    sizeSp: Int,
) {
    MaterialSymbol(
        name = "favorite",
        contentDescription = contentDescription,
        tint = tint,
        sizeSp = sizeSp,
        filled = favourite,
    )
}
