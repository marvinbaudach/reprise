package de.reprise.spike

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlin.math.roundToInt
import uniffi.reprise_android_ffi.AndroidArtworkSize

private const val VISUALIZER_CROSSFADE_MS = 120
private const val UNAVAILABLE_EXPLANATION = "Needs track analysis"

@Composable
internal fun NowPlayingVisualizer(
    trackUri: String,
    size: Int,
    shape: Shape,
) {
    val control = LocalVisualizerControl.current
    val visual = rememberTrackArtworkVisual(trackUri, AndroidArtworkSize.NOW_PLAYING)
    var menuOpen by remember { mutableStateOf(false) }
    var menuTouch by remember { mutableStateOf(Offset.Zero) }
    val haptics = LocalHapticFeedback.current

    Column {
        Box(
            modifier = Modifier
                .size(size.dp)
                .clip(shape)
                .testTag("visualizer-surface")
                .pointerInput(Unit) {
                    detectTapGestures(
                        onLongPress = { touch ->
                            menuTouch = touch
                            haptics.performHapticFeedback(HapticFeedbackType.LongPress)
                            menuOpen = true
                        },
                    )
                },
        ) {
            AnimatedContent(
                targetState = control.selected.renderedMode(),
                modifier = Modifier
                    .fillMaxSize()
                    .testTag("now-playing-cover"),
                transitionSpec = {
                    fadeIn(tween(VISUALIZER_CROSSFADE_MS)) togetherWith
                        fadeOut(tween(VISUALIZER_CROSSFADE_MS))
                },
                label = "now-playing-visualizer",
            ) { mode ->
                when (mode) {
                    MobileVisualizer.AMBIENT -> Box(
                        modifier = Modifier
                            .fillMaxSize()
                            .testTag("visualizer-ambient-surface"),
                    ) {
                        AmbientFields(visual?.ambientColors)
                    }
                    else -> Box(
                        modifier = Modifier
                            .fillMaxSize()
                            .testTag("visualizer-cover-surface"),
                    ) {
                        ArtworkCover(visual, size, shape = shape)
                    }
                }
            }
            Box(
                modifier = Modifier
                    .size(1.dp)
                    .offsetAt(menuTouch)
                    .testTag("visualizer-menu-anchor"),
            ) {
                VisualizerMenu(
                    expanded = menuOpen,
                    selected = control.selected,
                    dismiss = { menuOpen = false },
                    select = { mode ->
                        menuOpen = false
                        control.select(mode)
                    },
                )
            }
        }
        VisualizerBar(control)
    }
}

private fun Modifier.offsetAt(touch: Offset): Modifier = this.then(
    Modifier.offset { IntOffset(touch.x.roundToInt(), touch.y.roundToInt()) },
)

private fun MobileVisualizer.renderedMode(): MobileVisualizer =
    if (available) this else MobileVisualizer.COVER

@Composable
private fun VisualizerBar(control: VisualizerControl) {
    Row(
        modifier = Modifier
            .horizontalScroll(rememberScrollState())
            .padding(top = 8.dp),
    ) {
        MobileVisualizer.entries.forEach { mode ->
            TextButton(
                onClick = { control.select(mode) },
                enabled = mode.available,
                modifier = Modifier
                    .testTag("visualizer-bar-${mode.name}")
                    .semantics {
                        selected = mode == control.selected
                        role = Role.RadioButton
                    }
                    .then(
                        if (mode == control.selected) {
                            Modifier.background(
                                MaterialTheme.colorScheme.secondaryContainer,
                                MaterialTheme.shapes.large,
                            )
                        } else {
                            Modifier
                        },
                    ),
            ) {
                VisualizerModeLabel(mode)
            }
        }
    }
}

/**
 * A mode's name, and — when it cannot be chosen — the one line that says why.
 *
 * Shared by the always-visible bar and the long-press menu on purpose. The
 * explanation used to live in the menu alone, so a listener who never long-holds
 * saw two permanently greyed entries with no reason given, and absent-for-a-
 * reason is indistinguishable from broken. The line is a child of the button
 * itself rather than a tooltip, which is also how a screen reader gets it: the
 * button merges its descendants, so the mode is announced with its reason and
 * its disabled state in one go.
 */
@Composable
private fun VisualizerModeLabel(mode: MobileVisualizer) {
    Column {
        Text(mode.label, maxLines = 1)
        if (!mode.available) {
            Text(
                UNAVAILABLE_EXPLANATION,
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
            )
        }
    }
}

@Composable
private fun VisualizerMenu(
    expanded: Boolean,
    selected: MobileVisualizer,
    dismiss: () -> Unit,
    select: (MobileVisualizer) -> Unit,
) {
    DropdownMenu(expanded = expanded, onDismissRequest = dismiss) {
        MobileVisualizer.entries.forEach { mode ->
            DropdownMenuItem(
                text = { VisualizerModeLabel(mode) },
                leadingIcon = {
                    MaterialSymbol(
                        name = if (mode == selected) "radio_button_checked" else "radio_button_unchecked",
                        contentDescription = "",
                        sizeSp = 20,
                    )
                },
                enabled = mode.available,
                onClick = { select(mode) },
                modifier = Modifier
                    .testTag("visualizer-menu-${mode.name}")
                    .semantics {
                        this.selected = mode == selected
                        role = Role.RadioButton
                    },
            )
        }
    }
}
