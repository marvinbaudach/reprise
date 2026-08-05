package de.reprise.spike.settings

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import de.reprise.spike.MobileTheme
import de.reprise.spike.MobileThemeSelection

@Composable
internal fun AppearanceSettingsPage(
    themeSelection: MobileThemeSelection,
    selectTheme: (MobileTheme) -> Unit,
    back: () -> Unit,
) {
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
        }
    }
}

@Composable
private fun ThemeChoiceRow(
    theme: MobileTheme,
    selected: Boolean,
    select: () -> Unit,
) {
    Row(
        modifier = Modifier.fillMaxWidth(),
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
        RadioButton(selected = selected, onClick = select)
    }
}
