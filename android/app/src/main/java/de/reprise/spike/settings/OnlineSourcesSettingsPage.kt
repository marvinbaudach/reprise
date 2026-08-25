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
import de.reprise.spike.ArtistPhotoProgress
import de.reprise.spike.ArtistPhotoProgressBar

@Composable
internal fun OnlineSourcesSettingsPage(
    enabled: Boolean,
    setEnabled: (Boolean) -> Unit,
    progress: ArtistPhotoProgress? = null,
    dismissProgress: () -> Unit = {},
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
                Column {
                    SettingsSwitchRow(
                        title = "Download artist photos",
                        supporting = "Fetch portraits after a library scan or restore.",
                        checked = enabled,
                        onCheckedChange = setEnabled,
                    )
                    ArtistPhotoProgressBar(
                        progress = progress,
                        dismiss = dismissProgress,
                        inSettings = true,
                    )
                }
            }
            item {
                Text(
                    "Artist names in your library are sent to Deezer after a scan or restore. " +
                        "The app sends nothing else to the internet. " +
                        "With downloads off, album covers remain available.",
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    style = MaterialTheme.typography.bodyMedium,
                )
            }
        }
    }
}
