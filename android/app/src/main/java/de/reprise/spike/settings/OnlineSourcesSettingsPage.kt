package de.reprise.spike.settings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp

@Composable
internal fun OnlineSourcesSettingsPage(
    enabled: Boolean,
    setEnabled: (Boolean) -> Unit,
    back: () -> Unit,
) {
    Column(
        modifier = Modifier
            .fillMaxSize()
            .testTag("settings-page-online-sources"),
    ) {
        SettingsTopAppBar(
            title = "Online sources",
            backContentDescription = "Back to Settings",
            back = back,
        )
        LazyColumn(
            modifier = Modifier
                .fillMaxSize()
                .padding(horizontal = 16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            item { SettingsSectionTitle("Artist photos") }
            item {
                SettingsSwitchRow(
                    title = "Download artist photos",
                    supporting = "Fetch a portrait when you open an artist.",
                    checked = enabled,
                    onCheckedChange = setEnabled,
                )
            }
            item {
                Text(
                    "Each artist name whose page you open is sent to Deezer. " +
                        "The app sends nothing else to the internet. " +
                        "With downloads off, album covers remain available.",
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    style = MaterialTheme.typography.bodyMedium,
                )
            }
        }
    }
}
