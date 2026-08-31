package io.github.marvinbaudach.reprise

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp

@Composable
internal fun ArtistPhotoLibraryStatus(
    offerVisible: Boolean,
    downloadArtistPhotos: () -> Unit,
    declineArtistPhotos: () -> Unit,
    progress: ArtistPhotoProgress?,
    dismissProgress: () -> Unit,
) {
    if (offerVisible) {
        ArtistPhotoOfferBanner(downloadArtistPhotos, declineArtistPhotos)
    } else {
        ArtistPhotoProgressBar(progress, dismissProgress)
    }
}

@Composable
private fun ArtistPhotoOfferBanner(
    downloadArtistPhotos: () -> Unit,
    declineArtistPhotos: () -> Unit,
) {
    Surface(
        color = MaterialTheme.colorScheme.surfaceContainerHigh,
        shape = MaterialTheme.shapes.medium,
        border = BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant),
        modifier = Modifier
            .padding(horizontal = 12.dp)
            .padding(bottom = 8.dp)
            .fillMaxWidth()
            .testTag("artist-photo-offer"),
    ) {
        Column(
            modifier = Modifier.padding(horizontal = 12.dp, vertical = 10.dp),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            Text("Show artist photos?", style = MaterialTheme.typography.titleSmall)
            Text(
                "Reprise can download artist portraits from Deezer. " +
                    "Only artist names are sent, and album covers work without this.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.End,
            ) {
                TextButton(onClick = declineArtistPhotos) { Text("Not now") }
                TextButton(onClick = downloadArtistPhotos) { Text("Download artist photos") }
            }
        }
    }
}
