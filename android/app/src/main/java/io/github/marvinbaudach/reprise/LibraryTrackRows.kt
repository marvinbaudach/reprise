package io.github.marvinbaudach.reprise

import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.GridItemSpan
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.RectangleShape
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.positionChange
import androidx.compose.ui.layout.LocalPinnableContainer
import androidx.compose.ui.layout.boundsInRoot
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionInRoot
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.zIndex

internal data class QueueRowActions(
    val play: (position: Int, expectedTrackId: Long) -> Unit,
    val move: (fromPosition: Int, expectedTrackId: Long, toPosition: Int) -> Unit,
    val remove: (position: Int, expectedTrackId: Long) -> Unit,
)

/**
 * The library's track list: the 72 dp rows, their continuation sentinel, and
 * the badges the row carries. Shared by the Titles tab and by an opened album,
 * which is why it is not part of either.
 */
@Composable
internal fun TrackRows(
    surfaceLayout: SurfaceLayout,
    surfaceState: MobileSurfaceViewModel,
    listKey: LibraryListKey,
    tracks: LibraryWindow<LibraryTrack>,
    playback: LibraryPlayback,
    lastRequestedOffset: Long?,
    play: (Int) -> Unit,
    loadMore: (LibraryWindowRange) -> Unit,
    subtitle: TrackRowSubtitle = TrackRowSubtitle.ARTIST_AND_ALBUM,
    onFavouriteChanged: (LibraryTrack, Boolean) -> Unit = { _, _ -> },
    queueActions: QueueRowActions? = null,
    owner: String = "",
) {
    val metrics = libraryFrameMetrics(surfaceLayout)
    val content = trackListContent(tracks, lastRequestedOffset)
    val anchor = surfaceState.scrollPosition(listKey, owner).within(content.size)
    val rowKey: (TrackListContent) -> Any = if (queueActions == null) {
        TrackListContent::libraryRowKey
    } else {
        TrackListContent::queueRowKey
    }
    val reorder = rememberQueueReorderState()
    val haptics = rememberQueueHaptics()
    // The drop holds its offsets until the reloaded window agrees with them,
    // and this is how the window says so. Keyed by the order rather than by the
    // list instance, because every recomposition builds a fresh list.
    val order = content.joinToString(separator = ",") { item -> rowKey(item).toString() }
    LaunchedEffect(order) { reorder.onOrderChanged() }
    // Asked here rather than left to the effect above: by the time a
    // coroutine runs, the frame that double-counted the offsets is drawn.
    val offsetsHold = reorder.offsetsDescribe(order)

    if (surfaceLayout == SurfaceLayout.WIDE_SHORT) {
        val gridState = rememberLibraryGridState(anchor)
        ObserveLibraryGridAnchor(listKey, gridState, surfaceState, owner)
        SideEffect {
            reorder.windowOrder = order
            reorder.haptics = haptics
            reorder.move = queueActions?.move
            // A grid gives the drag neither a single column to part nor an
            // edge to scroll against: see QueueReorderState.neighboursPart.
            reorder.neighboursPart = metrics.listColumns == 1
            reorder.scrollPort = null
        }
        LazyVerticalGrid(
            columns = GridCells.Fixed(metrics.listColumns),
            state = gridState,
            modifier = Modifier
                .fillMaxSize()
                .testTag(listKey.testTag()),
            horizontalArrangement = Arrangement.spacedBy(metrics.listColumnGapDp.dp),
        ) {
            items(
                items = content,
                key = rowKey,
                span = { item ->
                    if (item is TrackListContent.Continuation) {
                        GridItemSpan(maxLineSpan)
                    } else {
                        GridItemSpan(1)
                    }
                },
            ) { item ->
                TrackListItem(
                    item,
                    surfaceState,
                    metrics,
                    playback,
                    play,
                    loadMore,
                    subtitle,
                    onFavouriteChanged,
                    queueActions,
                    tracks.rows.size,
                    reorder,
                    offsetsHold,
                )
            }
        }
        return
    }

    val listState = rememberLibraryListState(anchor)
    ObserveLibraryListAnchor(listKey, listState, surfaceState, owner)
    var viewportTopPx by remember { mutableFloatStateOf(0f) }
    var viewportBottomPx by remember { mutableFloatStateOf(0f) }
    val density = LocalDensity.current
    val scrollPort = remember(listState, density) {
        QueueScrollPort(
            viewportTopPx = { viewportTopPx },
            viewportBottomPx = { viewportBottomPx },
            topEdgePx = { with(density) { QUEUE_AUTOSCROLL_TOP_EDGE_DP.dp.toPx() } },
            bottomEdgePx = { with(density) { QUEUE_AUTOSCROLL_BOTTOM_EDGE_DP.dp.toPx() } },
            maxStepPx = { with(density) { QUEUE_AUTOSCROLL_MAX_STEP_DP.dp.toPx() } },
            scrollBy = { delta -> listState.dispatchRawDelta(delta) },
        )
    }
    SideEffect {
        reorder.windowOrder = order
        reorder.haptics = haptics
        reorder.move = queueActions?.move
        reorder.neighboursPart = true
        reorder.scrollPort = if (queueActions == null) null else scrollPort
    }
    LazyColumn(
        state = listState,
        modifier = Modifier
            .fillMaxSize()
            .testTag(listKey.testTag())
            .onGloballyPositioned { coordinates ->
                val bounds = coordinates.boundsInRoot()
                viewportTopPx = bounds.top
                viewportBottomPx = bounds.bottom
            },
    ) {
        items(
            items = content,
            key = rowKey,
        ) { content ->
            TrackListItem(
                content,
                surfaceState,
                metrics,
                playback,
                play,
                loadMore,
                subtitle,
                onFavouriteChanged,
                queueActions,
                tracks.rows.size,
                reorder,
                offsetsHold,
            )
        }
    }
}

/**
 * What identifies a row to the lazy list, and why the queue answers differently.
 *
 * A library list holds each track once, so the uri *is* the row: it survives
 * paging and re-sorting, and keeping it means a row keeps its item state while
 * the window around it grows.
 *
 * A queue slot is not a track. `Queue::enqueue` allows duplicates by design,
 * and "Play next" makes a second copy of one track a single tap away — at which
 * point a uri-only key is not merely imprecise, it throws
 * `IllegalArgumentException: Key … was already used` and takes the tab down.
 * The queue is therefore keyed by the slot, with the uri kept alongside so a
 * slot that changes hands does not inherit the previous occupant's row state.
 * Its window only ever grows by appending and is reloaded whole after every
 * edit, so the index is stable for exactly as long as the slot is.
 */
private fun TrackListContent.libraryRowKey(): String = when (this) {
    is TrackListContent.Row -> "track-${track.uri}"
    is TrackListContent.Continuation -> "load-window-${request.offset}"
}

private fun TrackListContent.queueRowKey(): String = when (this) {
    is TrackListContent.Row -> "queue-$index-${track.uri}"
    is TrackListContent.Continuation -> "load-window-${request.offset}"
}

internal fun LibraryListKey.testTag(): String = when (this) {
    LibraryListKey.TITLES -> "library-titles-list"
    LibraryListKey.ARTISTS -> "library-artists-list"
    LibraryListKey.ALBUM_TRACKS -> "library-album-tracks-list"
    LibraryListKey.ARTIST_ALBUMS -> "library-artist-albums-list"
    LibraryListKey.ARTIST_TRACKS -> "library-artist-tracks-list"
    LibraryListKey.UPCOMING -> "now-playing-queue"
}

internal enum class TrackRowSubtitle {
    ARTIST_AND_ALBUM,
    ALBUM_ONLY,
}

@Composable
internal fun TrackListItem(
    content: TrackListContent,
    surfaceState: MobileSurfaceViewModel,
    metrics: LibraryFrameMetrics,
    playback: LibraryPlayback,
    play: (Int) -> Unit,
    loadMore: (LibraryWindowRange) -> Unit,
    subtitle: TrackRowSubtitle,
    onFavouriteChanged: (LibraryTrack, Boolean) -> Unit,
    queueActions: QueueRowActions?,
    rowCount: Int,
    // Only a queue list has one; every other track list passes none, and no
    // row without [queueActions] ever reads it.
    reorder: QueueReorderState? = null,
    // False for the frames between the edit coming back and the offsets being
    // released: the reloaded window already carries the new order, so applying
    // them once more moves the row twice. See [QueueReorderState.offsetsDescribe].
    offsetsHold: Boolean = true,
) {
    when (content) {
        is TrackListContent.Row -> {
            val presentation = content.track.playbackPresentation(playback)
            val performanceObserver = LocalLibraryPerformanceObserver.current
            SideEffect {
                performanceObserver.trackRowComposed(content.track.id, presentation)
            }
            LibraryTrackRow(
                track = content.track,
                // The row a window was paged in with is a *copy* of the track, and
                // hearting it in Now Playing does not rewrite that copy. The rating is
                // therefore read from the one place all three surfaces read it —
                // only for the rows on screen, never for the whole window.
                surfaceState = surfaceState,
                presentation = presentation,
                metrics = metrics,
                subtitle = subtitle,
                onFavouriteChanged = onFavouriteChanged,
                queuePosition = content.index,
                queueRowCount = rowCount,
                queueActions = queueActions,
                reorder = reorder,
                offsetsHold = offsetsHold,
                play = { play(content.index) },
            )
        }
        is TrackListContent.Continuation -> {
            LaunchedEffect(content.request.offset) { loadMore(content.request) }
            LoadingWindowRow()
        }
    }
}

@Composable
private fun LibraryTrackRow(
    track: LibraryTrack,
    surfaceState: MobileSurfaceViewModel,
    presentation: TrackPlaybackPresentation,
    metrics: LibraryFrameMetrics,
    subtitle: TrackRowSubtitle,
    onFavouriteChanged: (LibraryTrack, Boolean) -> Unit,
    queuePosition: Int,
    queueRowCount: Int,
    queueActions: QueueRowActions?,
    reorder: QueueReorderState?,
    offsetsHold: Boolean,
    play: () -> Unit,
) {
    val contextMenu = rememberTrackContextMenuAnchorState()
    val queueDrag = if (queueActions == null) null else reorder
    val dragged = queueDrag?.isDragging(queuePosition) == true
    val shiftRows = if (offsetsHold) queueDrag?.neighbourShiftRows(queuePosition) ?: 0 else 0
    // One envelope for the whole lift, read by both the transform and the
    // colour: two animations of the same thing would drift apart.
    val lift = if (queueDrag == null) 0f else queueLiftFraction(dragged && queueDrag.lifted)
    KeepComposedWhileDragged(dragged)
    val restingColor = if (presentation.isCurrent) {
        MaterialTheme.colorScheme.primary.copy(alpha = 0.08f)
    } else {
        MaterialTheme.colorScheme.background
    }
    // The row itself is a fixed-height, clipped Surface, so the context menu's
    // acknowledgement gets a slot under it rather than a place on top of the
    // cover and the title. See TrackContextMenuMessage.
    Column(
        modifier = if (queueDrag == null) {
            Modifier
        } else {
            Modifier
                .zIndex(if (dragged) 1f else 0f)
                .queueDragMotion(
                    reorder = queueDrag,
                    dragged = dragged && offsetsHold,
                    lift = lift,
                    shiftRows = shiftRows,
                    rowHeightDp = metrics.trackRowHeightDp,
                )
        },
    ) {
        Surface(
            modifier = Modifier
                .fillMaxWidth()
                .height(metrics.trackRowHeightDp.dp)
                .clipToBounds()
                .testTag(
                    if (queueActions == null) {
                        "library-track-row-${track.id}"
                    } else {
                        "queue-track-row-${track.id}"
                    },
                )
                .trackContextMenuAnchor(contextMenu) {
                    if (queueActions == null) {
                        play()
                    } else {
                        queueActions.play(queuePosition, track.id)
                    }
                }
                .queueReorderActions(
                    actions = if (queueDrag == null) null else queueActions,
                    trackId = track.id,
                    position = queuePosition,
                    rowCount = queueRowCount,
                ),
            color = if (queueDrag == null) {
                restingColor
            } else {
                queueRowColor(restingColor, queueDrag, queuePosition, lift)
            },
        ) {
            Box {
                Row(
                    modifier = Modifier
                        .fillMaxSize()
                        .padding(horizontal = 16.dp, vertical = 8.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(16.dp),
                ) {
                    TrackCover(
                        trackUri = track.uri,
                        size = metrics.trackCoverSizeDp,
                        // The row is one clickable node, so anything described
                        // below it is merged into what the row announces. A cover
                        // saying "Album artwork" there replaces the song.
                        decorative = true,
                    )
                    Column(modifier = Modifier.weight(1f)) {
                        Text(
                            text = track.title,
                            style = MaterialTheme.typography.titleMedium,
                            color = if (presentation.isCurrent) {
                                MaterialTheme.colorScheme.onPrimaryContainer
                            } else {
                                MaterialTheme.colorScheme.onBackground
                            },
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                        val subtitleText = when (subtitle) {
                            TrackRowSubtitle.ARTIST_AND_ALBUM -> track.details()
                            TrackRowSubtitle.ALBUM_ONLY -> track.album.ifBlank { null }
                        }
                        if (subtitleText != null) {
                            Text(
                                text = subtitleText,
                                style = MaterialTheme.typography.bodyMedium,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                        }
                    }
                    if (queueDrag == null) {
                        FavouriteHeartButton(
                            track = track,
                            surfaceState = surfaceState,
                            onConfirmed = { favourite -> onFavouriteChanged(track, favourite) },
                        )
                    } else {
                        QueueDragHandle(
                            track = track,
                            position = queuePosition,
                            rowCount = queueRowCount,
                            rowHeightDp = metrics.trackRowHeightDp,
                            reorder = queueDrag,
                        )
                    }
                    Column(
                        modifier = Modifier.width(48.dp),
                        horizontalAlignment = Alignment.End,
                        verticalArrangement = Arrangement.spacedBy(2.dp),
                    ) {
                        if (presentation.isCurrent) {
                            PlayingBars(presentation.animateBars)
                        } else {
                            PlayCountBadge(track.playCount)
                        }
                        Text(
                            text = formatDuration(track.durationMs),
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                }
                HorizontalDivider(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(start = 88.dp)
                        .align(Alignment.BottomStart),
                    color = MaterialTheme.colorScheme.outlineVariant,
                )
                if (queueActions == null) {
                    TrackContextMenu(
                        anchor = contextMenu,
                        target = LibraryTrackMenuTarget(
                            label = track.title,
                            trackCount = 1,
                            resolveTrackIds = { listOf(track.id) },
                            play = { play() },
                        ),
                    )
                } else {
                    TrackContextMenu(
                        anchor = contextMenu,
                        target = QueueTrackMenuTarget(
                            trackId = track.id,
                            position = queuePosition,
                            actions = queueActions,
                        ),
                    )
                }
            }
        }
        TrackContextMenuMessage(contextMenu)
    }
}

/**
 * Keeps the row alive for as long as it is being carried.
 *
 * A lazy list composes what it can see. The dragged row stays under the finger
 * by translation alone — its *slot* is where it always was — so as soon as the
 * auto-scroll walks that slot past the edge of the viewport the list disposes
 * the row, and with it the pointer handler holding the gesture: the drag would
 * simply die halfway through, worse the further it travelled. Pinning is the
 * list's own answer to "this item has to outlive its visibility".
 */
@Composable
private fun KeepComposedWhileDragged(dragged: Boolean) {
    val container = LocalPinnableContainer.current
    DisposableEffect(dragged, container) {
        val pin = if (dragged) container?.pin() else null
        onDispose { pin?.release() }
    }
}

/** How lifted the row is, from flat to fully picked up. */
@Composable
private fun queueLiftFraction(lifted: Boolean): Float {
    val lift by animateFloatAsState(
        targetValue = if (lifted) 1f else 0f,
        // Picking up is immediate; putting down travels with the drop.
        animationSpec = tween(
            durationMillis = if (lifted) QUEUE_DRAG_LIFT_MS else QUEUE_DRAG_DROP_MS,
            easing = QueueDragEasing,
        ),
        label = "queue-drag-lift",
    )
    return lift
}

/**
 * Where a queue row is drawn while a reorder is in flight.
 *
 * One row is under the finger and follows it pixel for pixel; the rows between
 * its old and its new slot are one row height out of place and animate there.
 * Both are transforms, never layout: the list is not re-ordered until the edit
 * has actually been made, so nothing here can cost a measure pass.
 */
@Composable
private fun Modifier.queueDragMotion(
    reorder: QueueReorderState,
    dragged: Boolean,
    lift: Float,
    shiftRows: Int,
    rowHeightDp: Int,
): Modifier {
    val neighbourOffset by animateDpAsState(
        targetValue = (shiftRows * rowHeightDp).dp,
        animationSpec = tween(QUEUE_DRAG_NEIGHBOUR_MS, easing = QueueDragEasing),
        label = "queue-drag-neighbour",
    )
    return graphicsLayer {
        val scale = 1f + (QUEUE_DRAG_LIFT_SCALE - 1f) * lift
        translationY = if (dragged) reorder.translationPx else neighbourOffset.toPx()
        scaleX = scale
        scaleY = scale
        shadowElevation = QUEUE_DRAG_LIFT_ELEVATION_DP.dp.toPx() * lift
        shape = RectangleShape
        // The shadow belongs to the rows below, not inside this one.
        clip = false
    }
}

/**
 * The row's own colour, plus the two things a reorder adds to it: the lifted
 * row rises from the list's background onto a surface tone while it is held,
 * and the row that was moved keeps a short teal afterglow once it has landed.
 */
@Composable
private fun queueRowColor(
    resting: Color,
    reorder: QueueReorderState,
    slot: Int,
    lift: Float,
): Color {
    val held = lerp(resting, MaterialTheme.colorScheme.surface, lift)
    if (reorder.flashSlot != slot) {
        return held
    }
    // Only the one flashing row observes this per-frame animation value.
    val flash = reorder.flashFraction
    return lerp(
        held,
        MaterialTheme.colorScheme.primary,
        QUEUE_DRAG_FLASH_ALPHA * flash,
    )
}

/** TalkBack's label for the row's non-drag reorder, upwards and downwards. */
internal const val QUEUE_MOVE_UP_LABEL = "Move up"
internal const val QUEUE_MOVE_DOWN_LABEL = "Move down"

/**
 * The reorder a finger cannot make.
 *
 * The drag handle is the gesture, and it is the only one — a row cannot be
 * carried by a screen reader's focus. These two actions are the same permitted
 * move offered a second way (ACC-8, Android scope in `docs/ux-rules.md`): they
 * live in TalkBack's actions menu on the row, where the alternative to a drag
 * belongs, and nowhere on screen, so the handle stays the one discoverable way
 * to reorder for everyone who can reach it.
 *
 * They go through the same [QueueRowActions.move] as the drop does, so the
 * guards and the persistence path are the drag's. The ends of the queue simply
 * drop the action they have no room for.
 */
private fun Modifier.queueReorderActions(
    actions: QueueRowActions?,
    trackId: Long,
    position: Int,
    rowCount: Int,
): Modifier {
    if (actions == null) {
        return this
    }
    val moves = buildList {
        if (position > 0) {
            add(
                CustomAccessibilityAction(QUEUE_MOVE_UP_LABEL) {
                    actions.move(position, trackId, position - 1)
                    true
                },
            )
        }
        if (position + 1 < rowCount) {
            add(
                CustomAccessibilityAction(QUEUE_MOVE_DOWN_LABEL) {
                    actions.move(position, trackId, position + 1)
                    true
                },
            )
        }
    }
    return semantics { customActions = moves }
}

/**
 * The only way into a reorder.
 *
 * It takes the very first touch — no slop to cross and no long press to sit
 * through — because the handle has nothing else to do, and because a queue row
 * has two other gestures of its own: a tap plays it and a long press opens its
 * menu. Keeping the drag off the row's surface is what leaves those intact.
 */
@Composable
private fun QueueDragHandle(
    track: LibraryTrack,
    position: Int,
    rowCount: Int,
    rowHeightDp: Int,
    reorder: QueueReorderState,
) {
    val rowHeightPx = with(LocalDensity.current) { rowHeightDp.dp.toPx() }
    // Where the finger is on the screen, which is what the auto-scroll edges
    // are measured against. Read once at lift-off and carried forward by the
    // finger's own movement: the handle's layout position stops being the
    // truth the moment the row is translated out from under it.
    var handleTopPx by remember(track.id) { mutableFloatStateOf(0f) }
    Box(
        modifier = Modifier
            .width(48.dp)
            .height(48.dp)
            .testTag("queue-drag-handle-${track.id}")
            .onGloballyPositioned { coordinates ->
                handleTopPx = coordinates.positionInRoot().y
            }
            .pointerInput(track.id, position, rowCount, rowHeightPx) {
                awaitEachGesture {
                    val down = awaitFirstDown(requireUnconsumed = false)
                    // Consuming the down is what keeps the row's own clickable
                    // and the list's scroll out of the gesture.
                    down.consume()
                    reorder.begin(
                        slot = position,
                        trackId = track.id,
                        rowHeightPx = rowHeightPx,
                        slotCount = rowCount,
                        pointerRootYPx = handleTopPx + down.position.y,
                    )
                    var released = false
                    try {
                        while (true) {
                            val event = awaitPointerEvent()
                            val change = event.changes.firstOrNull { it.id == down.id } ?: break
                            if (!change.pressed) {
                                released = true
                                break
                            }
                            val delta = change.positionChange().y
                            change.consume()
                            if (delta != 0f) {
                                reorder.dragBy(delta)
                            }
                        }
                    } finally {
                        reorder.end(cancelled = !released)
                    }
                }
            },
        contentAlignment = Alignment.Center,
    ) {
        MaterialSymbol("drag_handle", "Reorder ${track.title}")
    }
}

@Composable
private fun PlayCountBadge(playCount: Long) {
    val normalizedPlayCount = playCount.coerceAtLeast(0)
    val description = pluralStringResource(
        R.plurals.play_count_description,
        normalizedPlayCount.coerceAtMost(Int.MAX_VALUE.toLong()).toInt(),
        normalizedPlayCount,
    )
    Surface(
        color = MaterialTheme.colorScheme.secondaryContainer,
        contentColor = MaterialTheme.colorScheme.onSecondaryContainer,
        shape = MaterialTheme.shapes.small,
    ) {
        Row(
            modifier = Modifier.padding(horizontal = 5.dp, vertical = 1.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            MaterialSymbol("play_arrow", description, sizeSp = 12)
            Text(normalizedPlayCount.toString(), style = MaterialTheme.typography.labelSmall)
        }
    }
}

/** What a window's continuation sentinel looks like while it loads. */
@Composable
internal fun LoadingWindowRow() {
    Text(
        text = "Loading…",
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        modifier = Modifier.padding(16.dp),
    )
}
