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
            item { SettingsSectionTitle("Artwork") }
            item {
                Text(
                    "Reprise downloads artist portraits from Deezer and album covers from " +
                        "MusicBrainz and the Cover Art Archive. It fetches after an automatic " +
                        "scan, a manual scan or a restore, and while an album without a cover " +
                        "of its own is playing. A cover is only fetched for an album that has " +
                        "none — one already showing art never triggers a request.",
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    style = MaterialTheme.typography.bodyMedium,
                )
            }
            item {
                Text(
                    "For that, artist names from your library are sent to Deezer and album " +
                        "titles to MusicBrainz. The app sends nothing else to the internet.",
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    style = MaterialTheme.typography.bodyMedium,
                )
            }
            item {
                ArtistPhotoProgressBar(
                    progress = progress,
                    dismiss = dismissProgress,
                    inSettings = true,
                )
            }
        }
    }
}
