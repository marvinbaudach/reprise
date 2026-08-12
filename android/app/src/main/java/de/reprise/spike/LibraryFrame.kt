package de.reprise.spike

import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.animateFloatAsState
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
import androidx.compose.foundation.layout.calculateStartPadding
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.fillMaxHeight
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
import androidx.compose.material3.NavigationRail
import androidx.compose.material3.NavigationRailDefaults
import androidx.compose.material3.NavigationRailItem
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
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import de.reprise.spike.ui.theme.MaterialSymbolsRounded
import de.reprise.spike.ui.theme.MaterialSymbolsRoundedFilled

@Composable
internal fun LibrarySummaryActions(
    summary: String,
    searching: Boolean,
    toggleSearch: () -> Unit,
    rescan: () -> Unit,
    openSettings: () -> Unit,
) {
    var menuExpanded by remember { mutableStateOf(false) }
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .height(48.dp)
            .testTag("library-summary-row")
            .padding(start = 16.dp, end = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            text = summary,
            style = MaterialTheme.typography.labelMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier
                .weight(1f)
                .testTag("library-summary-text"),
        )
        // Only while the field is shut. Once it is open it carries its own
        // trailing action — clear the text, then close — and a second cross
        // one row below it says the same thing twice.
        if (!searching) {
            IconButton(
                onClick = toggleSearch,
                modifier = Modifier.size(48.dp).testTag("library-summary-search"),
            ) {
                MaterialSymbol("search", "Search library")
            }
        }
        Box {
            IconButton(
                onClick = { menuExpanded = true },
                modifier = Modifier.size(48.dp).testTag("library-summary-overflow"),
            ) {
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
    surfaceLayout: SurfaceLayout,
    currentTrack: LibraryTrack?,
    playback: PlaybackUiState,
    selectedTab: BrowseTab,
    selectTab: (BrowseTab) -> Unit,
    openNowPlaying: () -> Unit,
    nowPlayingExpanded: Boolean = false,
) {
    val metrics = libraryFrameMetrics(surfaceLayout)
    val hiddenFraction by animateFloatAsState(
        targetValue = if (nowPlayingExpanded) 1f else 0f,
        label = "library-bottom-frame-visibility",
    )
    Column(
        modifier = Modifier.graphicsLayer {
            translationY = size.height * hiddenFraction
            alpha = 1f - hiddenFraction
        },
    ) {
        if (currentTrack != null) {
            MiniPlayer(
                metrics = metrics,
                track = currentTrack,
                playback = playback,
                openNowPlaying = openNowPlaying,
            )
        }
        if (surfaceLayout == SurfaceLayout.WIDE_SHORT) {
            return@Column
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
            modifier = Modifier
                .height(
                    metrics.navigationBarHeightDp.dp +
                    systemBarInsets.asPaddingValues().calculateBottomPadding(),
                )
                .testTag("library-navigation-bar"),
            containerColor = MaterialTheme.colorScheme.surface,
            windowInsets = systemBarInsets,
        ) {
            libraryDestinations.forEach { destination ->
                NavigationBarItem(
                    selected = destination == selectedTab,
                    onClick = { selectTab(destination) },
                    icon = { MaterialSymbol(destination.symbol, destination.label) },
                    label = { Text(destination.label) },
                    modifier = Modifier.testTag("library-destination-${destination.name}"),
                )
            }
        }
    }
}

/**
 * The wide-short arrangement's navigation, on the left edge.
 *
 * The active entry carries Material 3's own 56 × 32 dp indicator, which is the
 * pill 17a asks for. It is not declared a second time in
 * [LibraryFrameMetrics]: this component cannot be handed an indicator size, so
 * a constant would be a number the rail never reads and a test of it would only
 * prove that a constant equals itself.
 */
@Composable
internal fun LibraryNavigationRail(
    surfaceLayout: SurfaceLayout,
    selectedTab: BrowseTab,
    selectTab: (BrowseTab) -> Unit,
) {
    check(surfaceLayout == SurfaceLayout.WIDE_SHORT)
    val metrics = libraryFrameMetrics(surfaceLayout)
    // The same arithmetic the bottom bar needs below, on the other edge and for
    // the same reason: Material 3 pads the item column *inside* this component
    // by the system bar inset, so a bare 80 dp would be spent on the inset
    // rather than on the rail. Turned sideways that inset is where a
    // three-button navigation bar goes — wider than a rail item — and where a
    // rounded corner or a camera cut-out sits on the devices that have one.
    val systemBarInsets = NavigationRailDefaults.windowInsets
    val startInset = systemBarInsets
        .asPaddingValues()
        .calculateStartPadding(LocalLayoutDirection.current)
    NavigationRail(
        modifier = Modifier
            .width(metrics.navigationRailWidthDp.dp + startInset)
            .fillMaxHeight()
            .testTag("library-navigation-rail"),
        containerColor = MaterialTheme.colorScheme.surface,
        windowInsets = systemBarInsets,
    ) {
        Spacer(Modifier.weight(1f))
        libraryDestinations.forEach { destination ->
            NavigationRailItem(
                selected = destination == selectedTab,
                onClick = { selectTab(destination) },
                icon = { MaterialSymbol(destination.symbol, destination.label) },
                label = { Text(destination.label) },
                modifier = Modifier.testTag("library-destination-${destination.name}"),
            )
        }
        Spacer(Modifier.weight(1f))
    }
}

@Composable
private fun MiniPlayer(
    metrics: LibraryFrameMetrics,
    track: LibraryTrack,
    playback: PlaybackUiState,
    openNowPlaying: () -> Unit,
) {
    val controls = LocalPlaybackControls.current
    Surface(
        modifier = Modifier
            .fillMaxWidth()
            .height(metrics.miniPlayerHeightDp.dp)
            .testTag("library-mini-player")
            .padding(horizontal = 12.dp)
            // The label names the *action*; it does not replace what this node
            // announces. A content description would: it wins over the merged
            // descendants, and the one thing a screen-reader user needs here is
            // which track is playing.
            .clickable(onClickLabel = "Open Now Playing", onClick = openNowPlaying),
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
                    size = metrics.trackCoverSizeDp,
                    decorative = true,
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
                        name = playback.playPauseSymbol,
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
internal fun CoverPlaceholder(
    size: Int,
    shape: androidx.compose.ui.graphics.Shape? = null,
    decorative: Boolean = false,
    modifier: Modifier = Modifier,
) {
    Box(
        modifier = modifier
            .size(size.dp)
            .clip(shape ?: MaterialTheme.shapes.small)
            .background(MaterialTheme.colorScheme.surfaceContainerHigh),
        contentAlignment = Alignment.Center,
    ) {
        MaterialSymbol(
            name = "music_note",
            contentDescription = if (decorative) "" else "No artwork",
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

/**
 * One Material Symbols glyph, named by its ligature.
 *
 * [filled] is a *separate* parameter and not another [name] on purpose: fill is
 * a variable-font axis in this font, so the "filled" and "outlined" spellings of
 * a symbol are one and the same glyph — see [MaterialSymbolsRoundedFilled].
 * A caller that carried an on/off state in the name drew the same pixels twice.
 */
@Composable
internal fun MaterialSymbol(
    name: String,
    contentDescription: String,
    tint: Color = LocalContentColor.current,
    sizeSp: Int = 24,
    filled: Boolean = false,
) {
    Text(
        text = name,
        color = tint,
        fontFamily = if (filled) MaterialSymbolsRoundedFilled else MaterialSymbolsRounded,
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
