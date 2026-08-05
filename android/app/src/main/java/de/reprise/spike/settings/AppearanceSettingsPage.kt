package de.reprise.spike.settings

import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import de.reprise.spike.LocalVisualizerControl
import de.reprise.spike.LocalLibraryRatingControl
import de.reprise.spike.MobileTheme
import de.reprise.spike.MobileThemeSelection
import de.reprise.spike.MobileVisualizer

@Composable
internal fun AppearanceSettingsPage(
    themeSelection: MobileThemeSelection,
    selectTheme: (MobileTheme) -> Unit,
    back: () -> Unit,
) {
    val visualizer = LocalVisualizerControl.current
    val libraryRating = LocalLibraryRatingControl.current
    Column(modifier = Modifier.fillMaxSize()) {
        SettingsTopAppBar(
            title = "Appearance",
            backContentDescription = "Back to Settings",
            back = back,
        )
        LazyColumn(
            modifier = Modifier
                .fillMaxSize()
                .padding(horizontal = 16.dp),
        ) {
            item { SettingsSectionTitle("Theme") }
            items(themeSelection.availableThemes) { theme ->
                ThemeChoiceRow(
                    theme = theme,
                    selected = themeSelection.palette == theme,
                    select = { selectTheme(theme) },
                )
            }
            item { SettingsSectionTitle("Visualizer") }
            items(MobileVisualizer.entries) { mode ->
                VisualizerChoiceRow(
                    mode = mode,
                    selected = visualizer.selected == mode,
                    select = { visualizer.select(mode) },
                )
            }
            item { SettingsSectionTitle("Library") }
            item {
                LibraryRatingRow(
                    checked = libraryRating.enabled,
                    select = libraryRating.select,
                )
            }
        }
    }
}

@Composable
private fun LibraryRatingRow(
    checked: Boolean,
    select: (Boolean) -> Unit,
) {
    // The row is the target, not the switch at its edge — see the note on
    // `SettingsSwitchRow`. The tag rides on the row for the same reason: it is
    // the node that now carries the state and the click.
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .toggleable(
                value = checked,
                onValueChange = select,
                role = Role.Switch,
            )
            .testTag("settings-library-rating"),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(modifier = Modifier.weight(1f)) {
            Text("Show ratings in library rows")
            Text(
                "Display each track's star rating.",
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                style = MaterialTheme.typography.bodyMedium,
            )
        }
        Switch(checked = checked, onCheckedChange = null)
    }
}

@Composable
private fun VisualizerChoiceRow(
    mode: MobileVisualizer,
    selected: Boolean,
    select: () -> Unit,
) {
    // The row is the target. A disabled choice stays visibly disabled: the row
    // takes no click either, so "Needs track analysis" is the whole answer
    // rather than a label above a control that quietly does nothing.
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .selectable(
                selected = selected,
                enabled = mode.available,
                onClick = select,
                role = Role.RadioButton,
            )
            .testTag("settings-visualizer-${mode.name}"),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(modifier = Modifier.weight(1f)) {
            Text(mode.label)
            if (!mode.available) {
                Text(
                    "Needs track analysis",
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    style = MaterialTheme.typography.bodyMedium,
                )
            }
        }
        RadioButton(selected = selected, enabled = mode.available, onClick = null)
    }
}

@Composable
private fun ThemeChoiceRow(
    theme: MobileTheme,
    selected: Boolean,
    select: () -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .selectable(selected = selected, onClick = select, role = Role.RadioButton),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(modifier = Modifier.weight(1f)) {
            Text(theme.displayName())
            Text(
                when (theme) {
                    MobileTheme.NOCTURNE -> "Reprise's dark palette"
                    MobileTheme.DYNAMIC -> "Colours from this device"
                },
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                style = MaterialTheme.typography.bodyMedium,
            )
        }
        RadioButton(selected = selected, onClick = null)
    }
}
