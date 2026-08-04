package de.reprise.spike

import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.asPaddingValues
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.IconButton
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarDefaults
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import de.reprise.spike.ui.theme.MaterialSymbolsRounded

@Composable
internal fun LibraryTopAppBar(
    searching: Boolean,
    toggleSearch: () -> Unit,
    rescan: () -> Unit,
    chooseFolder: () -> Unit,
    openSettings: () -> Unit,
) {
    var menuExpanded by remember { mutableStateOf(false) }
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .height(libraryFrameMetrics.topAppBarHeightDp.dp)
            .padding(start = 16.dp, end = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            text = "Library",
            style = MaterialTheme.typography.titleLarge,
            modifier = Modifier.weight(1f),
        )
        IconButton(onClick = toggleSearch, modifier = Modifier.size(48.dp)) {
            MaterialSymbol(
                name = if (searching) "close" else "search",
                contentDescription = if (searching) "Close search" else "Search library",
            )
        }
        Box {
            IconButton(onClick = { menuExpanded = true }, modifier = Modifier.size(48.dp)) {
                MaterialSymbol("more_vert", "Library actions")
            }
            DropdownMenu(
                expanded = menuExpanded,
                onDismissRequest = { menuExpanded = false },
            ) {
                DropdownMenuItem(
                    text = { Text("Rescan") },
                    leadingIcon = { MaterialSymbol("refresh", "") },
                    onClick = {
                        menuExpanded = false
                        rescan()
                    },
                )
                DropdownMenuItem(
                    text = { Text("Choose another folder") },
                    leadingIcon = { MaterialSymbol("folder_open", "") },
                    onClick = {
                        menuExpanded = false
                        chooseFolder()
                    },
                )
                DropdownMenuItem(
                    text = { Text("Settings") },
                    leadingIcon = { MaterialSymbol("settings", "") },
                    onClick = {
                        menuExpanded = false
                        openSettings()
                    },
                )
            }
        }
    }
}

@Composable
internal fun LibraryBottomFrame(
    currentTrack: LibraryTrack?,
    playback: PlaybackUiState,
    openNowPlaying: () -> Unit,
) {
    Column {
        if (currentTrack != null) {
            MiniPlayer(
                track = currentTrack,
                playback = playback,
                openNowPlaying = openNowPlaying,
            )
        }
        // Material 3 pads the item row *inside* this component by the system
        // bar inset, so a bare 80 dp on the outside would be spent on the
        // inset instead of on the bar: with gesture navigation ~19 dp of it,
        // with three buttons ~48 dp, which is less than one active pill needs.
        // Adding the very same inset to the height leaves the plan's 80 dp of
        // bar intact and puts the system's area below it. The root consumes
        // only the status bar, so this is the one place the bottom inset is
        // spent.
        val systemBarInsets = NavigationBarDefaults.windowInsets
        NavigationBar(
            modifier = Modifier.height(
                libraryFrameMetrics.navigationBarHeightDp.dp +
                    systemBarInsets.asPaddingValues().calculateBottomPadding(),
            ),
            containerColor = MaterialTheme.colorScheme.surface,
            windowInsets = systemBarInsets,
        ) {
            libraryDestinations.forEach { destination ->
                NavigationBarItem(
                    selected = true,
                    onClick = {},
                    icon = { MaterialSymbol(destination.symbol, destination.label) },
                    label = { Text(destination.label) },
                )
            }
        }
    }
}

@Composable
private fun MiniPlayer(
    track: LibraryTrack,
    playback: PlaybackUiState,
    openNowPlaying: () -> Unit,
) {
    val controls = LocalPlaybackControls.current
    Surface(
        modifier = Modifier
            .fillMaxWidth()
            .height(libraryFrameMetrics.miniPlayerHeightDp.dp)
            .padding(horizontal = 12.dp)
            .clickable(onClick = openNowPlaying)
            .semantics { contentDescription = "Open Now Playing" },
        color = MaterialTheme.colorScheme.surfaceContainer,
        shape = MaterialTheme.shapes.large,
    ) {
        Box {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                TrackCover(
                    trackUri = track.uri,
                    size = libraryFrameMetrics.trackCoverSizeDp,
                )
                Spacer(Modifier.width(12.dp))
                Column(modifier = Modifier.weight(1f)) {
                    Text(
                        text = track.title,
                        style = MaterialTheme.typography.titleMedium,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                    Text(
                        text = track.artist.ifBlank { "Unknown artist" },
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
                IconButton(onClick = controls::previous, modifier = Modifier.size(48.dp)) {
                    MaterialSymbol("skip_previous", "Previous track")
                }
                IconButton(
                    onClick = controls::togglePause,
                    modifier = Modifier
                        .size(48.dp)
                        .clip(MaterialTheme.shapes.large)
                        .background(MaterialTheme.colorScheme.primary),
                ) {
                    MaterialSymbol(
                        name = if (playback.isPlaying) "pause" else "play_arrow",
                        contentDescription = playback.playPauseLabel,
                        tint = MaterialTheme.colorScheme.onPrimary,
                        sizeSp = 30,
                    )
                }
                IconButton(onClick = controls::next, modifier = Modifier.size(48.dp)) {
                    MaterialSymbol("skip_next", "Next track")
                }
            }
            Box(
                modifier = Modifier
                    .fillMaxWidth()
                    .height(3.dp)
                    .align(Alignment.BottomStart)
                    .background(MaterialTheme.colorScheme.outlineVariant),
            )
            Box(
                modifier = Modifier
                    .fillMaxWidth(playback.progressFraction)
                    .height(3.dp)
                    .align(Alignment.BottomStart)
                    .background(MaterialTheme.colorScheme.primary),
            )
        }
    }
}

@Composable
internal fun CoverPlaceholder(size: Int, shape: androidx.compose.ui.graphics.Shape? = null) {
    Box(
        modifier = Modifier
            .size(size.dp)
            .clip(shape ?: MaterialTheme.shapes.small)
            .background(MaterialTheme.colorScheme.surfaceContainerHigh),
        contentAlignment = Alignment.Center,
    ) {
        MaterialSymbol(
            name = "music_note",
            contentDescription = "No artwork",
            tint = MaterialTheme.colorScheme.onSurfaceVariant,
            sizeSp = 28,
        )
    }
}

@Composable
internal fun PlayingBars(animate: Boolean) {
    val transition = rememberInfiniteTransition(label = "playing-row-bars")
    val heights = listOf(10f, 16f, 7f, 13f).mapIndexed { index, target ->
        if (!animate) {
            target
        } else {
            val height by transition.animateFloat(
                initialValue = 4f + index,
                targetValue = target,
                animationSpec = infiniteRepeatable(
                    animation = tween(durationMillis = 420 + index * 90),
                    repeatMode = RepeatMode.Reverse,
                ),
                label = "playing-row-bar-$index",
            )
            height
        }
    }
    Row(
        modifier = Modifier
            .width(24.dp)
            .height(16.dp),
        horizontalArrangement = Arrangement.spacedBy(3.dp),
        verticalAlignment = Alignment.Bottom,
    ) {
        heights.forEach { height ->
            Box(
                modifier = Modifier
                    .width(3.dp)
                    .height(height.dp)
                    .background(
                        MaterialTheme.colorScheme.primary,
                        RoundedCornerShape(2.dp),
                    ),
            )
        }
    }
}

@Composable
internal fun MaterialSymbol(
    name: String,
    contentDescription: String,
    tint: Color = LocalContentColor.current,
    sizeSp: Int = 24,
) {
    Text(
        text = name,
        color = tint,
        fontFamily = MaterialSymbolsRounded,
        fontSize = sizeSp.sp,
        lineHeight = sizeSp.sp,
        maxLines = 1,
        modifier = Modifier
            .widthIn(min = sizeSp.dp)
            .then(
                if (contentDescription.isBlank()) {
                    Modifier.clearAndSetSemantics {}
                } else {
                    Modifier.clearAndSetSemantics {
                        this.contentDescription = contentDescription
                    }
                },
            ),
    )
}
