package de.reprise.spike

import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.util.Log
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import uniffi.reprise_android_ffi.MusicLibrary
import uniffi.reprise_android_ffi.ScanProgressListener
import uniffi.reprise_android_ffi.ScanProgressUpdate

private const val TAG = "RepriseScan"

class MainActivity : ComponentActivity() {
    private val libraryDelegate = lazy { MusicLibrary.open(filesDir.absolutePath) }
    private val library by libraryDelegate

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            MaterialTheme {
                Surface(modifier = Modifier.fillMaxSize()) {
                    ScanRunner(::scanTree)
                }
            }
        }
    }

    override fun onDestroy() {
        if (libraryDelegate.isInitialized()) {
            libraryDelegate.value.close()
        }
        super.onDestroy()
    }

    private fun scanTree(treeUri: Uri, report: (String) -> Unit) {
        runCatching {
            contentResolver.takePersistableUriPermission(
                treeUri,
                Intent.FLAG_GRANT_READ_URI_PERMISSION,
            )
        }.onFailure { error ->
            val message =
                "Persisting folder permission failed: ${error::class.java.simpleName}: " +
                    error.message
            Log.e(TAG, message, error)
            report(message)
            return
        }
        report("Scanning selected folder…")
        Thread {
            val outcome = runCatching {
                library.setTreeUri(
                    treeUri.toString(),
                    AndroidSafSource(contentResolver, treeUri),
                )
                val summary = library.scan(LogProgress())
                val tracks = library.listTracks()
                "Scan call returned: tracks=${tracks.size} added=${summary.added} " +
                    "updated=${summary.updated} errors=${summary.errors}"
            }
            runOnUiThread {
                outcome.fold(
                    onSuccess = { message ->
                        Log.i(TAG, message)
                        report(message)
                    },
                    onFailure = { error ->
                        val message =
                            "Scan call failed: ${error::class.java.simpleName}: ${error.message}"
                        Log.e(TAG, message, error)
                        report(message)
                    },
                )
            }
        }.start()
    }
}

private class LogProgress : ScanProgressListener {
    override fun onProgress(progress: ScanProgressUpdate) {
        when (progress) {
            ScanProgressUpdate.Discovering -> Log.i(TAG, "Scan discovery started")
            is ScanProgressUpdate.Scanning -> {
                val total = progress.total?.toString() ?: "unknown"
                Log.i(
                    TAG,
                    "Scan progress: processed=${progress.processed} total=$total " +
                        "uri=${progress.currentUri}",
                )
            }
            is ScanProgressUpdate.Fetching -> Log.i(
                TAG,
                "Scan fetch progress: done=${progress.done} total=${progress.total}",
            )
        }
    }
}

@Composable
private fun ScanRunner(scan: (Uri, (String) -> Unit) -> Unit) {
    var outcome by remember { mutableStateOf("Choose a folder to prepare the Phase 3 device run.") }
    val folderPicker = rememberLauncherForActivityResult(
        contract = ActivityResultContracts.OpenDocumentTree(),
    ) { treeUri ->
        if (treeUri == null) {
            outcome = "Folder selection cancelled."
        } else {
            scan(treeUri) { message -> outcome = message }
        }
    }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp, Alignment.CenterVertically),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Button(onClick = { folderPicker.launch(null) }) {
            Text("Choose folder for Phase 3")
        }
        Text(outcome, style = MaterialTheme.typography.bodyMedium)
    }
}
