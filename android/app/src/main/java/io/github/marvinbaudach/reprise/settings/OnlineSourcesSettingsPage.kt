package io.github.marvinbaudach.reprise.settings

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
import io.github.marvinbaudach.reprise.ArtistPhotoProgress
import io.github.marvinbaudach.reprise.ArtistPhotoProgressBar

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
                        supporting = "Fetch portraits and album covers after automatic scans, " +
                            "manual scans, or restores, and while an album without its own " +
                            "cover is playing.",
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
                    "Artist names in your library are sent to Deezer for portraits, and " +
                        "album titles to MusicBrainz and the Cover Art Archive for covers, " +
                        "after an automatic scan, manual scan, or restore. " +
                        "A cover is only fetched for an album that has none of its own — one " +
                        "already showing art never triggers a request. " +
                        "The app sends nothing else to the internet. " +
                        "With this off, only artwork already in your files or folders shows.",
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    style = MaterialTheme.typography.bodyMedium,
                )
            }
        }
    }
}
