package de.reprise.spike

import android.net.Uri
import android.os.Bundle
import android.provider.DocumentsContract
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
import uniffi.reprise_android_ffi.FileDescriptorProbeResult
import uniffi.reprise_android_ffi.probeFileDescriptor

private const val TAG = "RepriseA1"

class MainActivity : ComponentActivity() {

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            MaterialTheme {
                Surface(modifier = Modifier.fillMaxSize()) {
                    ProbeScreen(::probeFirstFile)
                }
            }
        }
    }

    private fun probeFirstFile(treeUri: Uri): String = try {
        val fileUri = firstFileIn(treeUri) ?: error("The selected folder contains no file")
        val result = contentResolver.openFileDescriptor(fileUri, "r")?.use { descriptor ->
            // detachFd transfers ownership; the Rust probe adopts and closes it.
            probeFileDescriptor(descriptor.detachFd())
        } ?: error("The provider returned no file descriptor")

        result.logSummary().also { summary -> Log.i(TAG, summary) }
    } catch (error: Throwable) {
        "A1 probe failed: ${error::class.java.simpleName}: ${error.message}".also { summary ->
            Log.e(TAG, summary, error)
        }
    }

    private fun firstFileIn(treeUri: Uri): Uri? {
        val treeDocumentId = DocumentsContract.getTreeDocumentId(treeUri)
        val childrenUri =
            DocumentsContract.buildChildDocumentsUriUsingTree(treeUri, treeDocumentId)
        val projection = arrayOf(
            DocumentsContract.Document.COLUMN_DOCUMENT_ID,
            DocumentsContract.Document.COLUMN_MIME_TYPE,
        )

        contentResolver.query(childrenUri, projection, null, null, null)?.use { cursor ->
            val idColumn = cursor.getColumnIndexOrThrow(
                DocumentsContract.Document.COLUMN_DOCUMENT_ID,
            )
            val mimeColumn = cursor.getColumnIndexOrThrow(
                DocumentsContract.Document.COLUMN_MIME_TYPE,
            )
            while (cursor.moveToNext()) {
                if (cursor.getString(mimeColumn) != DocumentsContract.Document.MIME_TYPE_DIR) {
                    return DocumentsContract.buildDocumentUriUsingTree(
                        treeUri,
                        cursor.getString(idColumn),
                    )
                }
            }
        }
        return null
    }
}

private fun FileDescriptorProbeResult.logSummary(): String =
    "A1 descriptor probe: bytesRead=$bytesRead readError=$readError " +
        "seekSucceeded=$seekSucceeded seekError=$seekError " +
        "bytesReadAfterSeek=$bytesReadAfterSeek " +
        "readAfterSeekError=$readAfterSeekError bytesMatch=$bytesMatch"

@Composable
private fun ProbeScreen(probe: (Uri) -> String) {
    var outcome by remember { mutableStateOf("Choose a folder to run the A1 descriptor probe.") }
    val folderPicker = rememberLauncherForActivityResult(
        contract = ActivityResultContracts.OpenDocumentTree(),
    ) { treeUri ->
        outcome = treeUri?.let(probe) ?: "Folder selection cancelled."
    }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp, Alignment.CenterVertically),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Button(onClick = { folderPicker.launch(null) }) {
            Text("Choose folder and run A1")
        }
        Text(outcome, style = MaterialTheme.typography.bodyMedium)
    }
}
