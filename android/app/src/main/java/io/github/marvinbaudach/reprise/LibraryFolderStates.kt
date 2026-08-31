package io.github.marvinbaudach.reprise

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp

@Composable
internal fun TreeUnreadableScreen(chooseFolder: () -> Unit) {
    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp, Alignment.CenterVertically),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text(
            "Reprise can no longer read the saved music folder. " +
                "Access may have been revoked or the folder may have been removed.",
        )
        Button(onClick = chooseFolder) {
            Text("Choose folder again")
        }
    }
}

@Composable
internal fun NoFolderScreen(message: String?, chooseFolder: () -> Unit) {
    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp, Alignment.CenterVertically),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text(
            "Choose Music/Reprise to build this device's library. " +
                "If it is not available, choose Music.",
        )
        Button(onClick = chooseFolder) {
            Text("Choose folder")
        }
        message?.let { Text(it, color = MaterialTheme.colorScheme.error) }
    }
}

@Composable
internal fun ScanningScreen(state: LibraryScreenState.Scanning) {
    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp, Alignment.CenterVertically),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text(
            state.total?.let { total -> "Scanning ${state.processed} of $total…" }
                ?: "Scanning… ${state.processed} found",
        )
        when (val progress = state.progressPresentation()) {
            ScanProgressPresentation.Indeterminate -> LinearProgressIndicator(
                modifier = Modifier.fillMaxWidth(),
            )
            is ScanProgressPresentation.Determinate -> LinearProgressIndicator(
                progress = { progress.fraction },
                modifier = Modifier.fillMaxWidth(),
            )
        }
    }
}
