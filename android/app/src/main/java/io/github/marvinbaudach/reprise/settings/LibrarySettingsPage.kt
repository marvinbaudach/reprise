package io.github.marvinbaudach.reprise.settings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material3.Button
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import io.github.marvinbaudach.reprise.MaterialSymbol

@Composable
internal fun LibrarySettingsPage(
    titleCount: Long,
    albumCount: Long,
    artistCount: Long,
    folderName: String?,
    back: () -> Unit,
    chooseFolder: () -> Unit,
    rescan: () -> Unit,
) {
    Column(modifier = Modifier.fillMaxSize()) {
        SettingsTopAppBar(
            title = "Library & scan folder",
            backContentDescription = "Back to Settings",
            back = back,
        )
        LazyColumn(
            modifier = Modifier
                .fillMaxSize()
                .padding(horizontal = 16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            item { SettingsSectionTitle("Scan folder") }
            item {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text("Selected music folder", style = MaterialTheme.typography.bodyLarge)
                    // The name, or the count when the provider's document id is
                    // not a path — see `folderLabel`. Printing an opaque token
                    // would be worse than saying nothing.
                    Text(
                        folderName ?: "1 folder",
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        style = MaterialTheme.typography.bodyMedium,
                    )
                    Button(onClick = chooseFolder) {
                        Text("Choose another folder")
                    }
                }
            }
            item { SettingsSectionTitle("Library") }
            item {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(
                        listOf(
                            countLabel(titleCount, "title", "titles"),
                            countLabel(albumCount, "album", "albums"),
                            countLabel(artistCount, "artist", "artists"),
                        ).joinToString(" · "),
                        modifier = Modifier.weight(1f),
                        style = MaterialTheme.typography.bodyLarge,
                    )
                    IconButton(onClick = rescan) {
                        MaterialSymbol("refresh", "Rescan library")
                    }
                }
            }
        }
    }
}

private fun countLabel(total: Long, singular: String, plural: String): String =
    "$total ${if (total == 1L) singular else plural}"
