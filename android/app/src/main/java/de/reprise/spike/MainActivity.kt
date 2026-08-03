package de.reprise.spike

import android.content.Intent
import android.media.MediaPlayer
import android.net.Uri
import android.os.Bundle
import android.util.Log
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Button
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.ListItem
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
    private var player: MediaPlayer? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            MaterialTheme {
                Surface(modifier = Modifier.fillMaxSize()) {
                    LibraryScreen(
                        chooseFolder = ::scanTree,
                        playTrack = ::playTrack,
                    )
                }
            }
        }
    }

    override fun onDestroy() {
        releasePlayer()
        if (libraryDelegate.isInitialized()) {
            libraryDelegate.value.close()
        }
        super.onDestroy()
    }

    private fun scanTree(treeUri: Uri, report: (LibraryScreenState) -> Unit) {
        runCatching {
            contentResolver.takePersistableUriPermission(
                treeUri,
                Intent.FLAG_GRANT_READ_URI_PERMISSION,
            )
        }.onFailure { error ->
            val message = "Could not keep access to the selected folder: ${error.detail()}"
            Log.e(TAG, message, error)
            report(LibraryScreenState.NoFolder(message))
            return
        }

        report(LibraryScreenState.Scanning())
        Thread {
            val outcome = runCatching {
                library.setTreeUri(
                    treeUri.toString(),
                    AndroidSafSource(contentResolver, treeUri),
                )
                val summary = library.scan(
                    UiProgress { progress ->
                        runOnUiThread { report(progress) }
                    },
                )
                val tracks = library.listTracks().map { track ->
                    LibraryTrack(
                        uri = track.uri,
                        title = track.title,
                        artist = track.artist,
                        album = track.album,
                        durationMs = track.durationMs,
                    )
                }
                Log.i(
                    TAG,
                    "Scan completed: tracks=${tracks.size} added=${summary.added} " +
                        "updated=${summary.updated} errors=${summary.errors}",
                )
                LibraryScreenState.TrackList(tracks)
            }
            runOnUiThread {
                outcome.fold(
                    onSuccess = report,
                    onFailure = { error ->
                        val message = "Could not scan the selected folder: ${error.detail()}"
                        Log.e(TAG, message, error)
                        report(LibraryScreenState.NoFolder(message))
                    },
                )
            }
        }.start()
    }

    private fun playTrack(track: LibraryTrack, reportError: (String) -> Unit) {
        releasePlayer()
        val next = MediaPlayer()
        player = next
        next.setOnPreparedListener { prepared ->
            if (player === prepared) {
                prepared.start()
            } else {
                prepared.release()
            }
        }
        next.setOnCompletionListener { completed ->
            if (player === completed) {
                player = null
            }
            completed.release()
        }
        next.setOnErrorListener { failed, what, extra ->
            if (player === failed) {
                player = null
            }
            failed.release()
            reportError("Could not play ${track.title} (MediaPlayer $what/$extra).")
            true
        }
        runCatching {
            next.setDataSource(this, Uri.parse(track.uri))
            next.prepareAsync()
        }.onFailure { error ->
            if (player === next) {
                player = null
            }
            next.release()
            reportError("Could not play ${track.title}: ${error.detail()}")
        }
    }

    private fun releasePlayer() {
        player?.release()
        player = null
    }
}

private class UiProgress(
    private val report: (LibraryScreenState.Scanning) -> Unit,
) : ScanProgressListener {
    override fun onProgress(progress: ScanProgressUpdate) {
        val scanning = when (progress) {
            ScanProgressUpdate.Discovering -> LibraryScreenState.Scanning()
            is ScanProgressUpdate.Scanning -> LibraryScreenState.Scanning(
                processed = progress.processed,
                total = progress.total,
            )
            is ScanProgressUpdate.Fetching -> LibraryScreenState.Scanning(
                processed = progress.done,
                total = progress.total,
            )
        }
        report(scanning)
    }
}

@Composable
private fun LibraryScreen(
    chooseFolder: (Uri, (LibraryScreenState) -> Unit) -> Unit,
    playTrack: (LibraryTrack, (String) -> Unit) -> Unit,
) {
    var state by remember { mutableStateOf<LibraryScreenState>(LibraryScreenState.NoFolder()) }
    val folderPicker = rememberLauncherForActivityResult(
        contract = ActivityResultContracts.OpenDocumentTree(),
    ) { treeUri ->
        if (treeUri != null) {
            chooseFolder(treeUri) { state = it }
        }
    }

    when (val current = state) {
        is LibraryScreenState.NoFolder -> NoFolderScreen(
            message = current.message,
            chooseFolder = { folderPicker.launch(null) },
        )
        is LibraryScreenState.Scanning -> ScanningScreen(current)
        is LibraryScreenState.TrackList -> TrackListScreen(
            state = current,
            chooseFolder = { folderPicker.launch(null) },
            playTrack = { track ->
                state = current.copy(message = null)
                playTrack(track) { message ->
                    val visible = state
                    if (visible is LibraryScreenState.TrackList) {
                        state = visible.copy(message = message)
                    }
                }
            },
        )
    }
}

@Composable
private fun NoFolderScreen(message: String?, chooseFolder: () -> Unit) {
    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp, Alignment.CenterVertically),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text("Choose a music folder to build this device's library.")
        Button(onClick = chooseFolder) {
            Text("Choose folder")
        }
        message?.let { Text(it, color = MaterialTheme.colorScheme.error) }
    }
}

@Composable
private fun ScanningScreen(state: LibraryScreenState.Scanning) {
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

@Composable
private fun TrackListScreen(
    state: LibraryScreenState.TrackList,
    chooseFolder: () -> Unit,
    playTrack: (LibraryTrack) -> Unit,
) {
    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = 16.dp, vertical = 20.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Button(onClick = chooseFolder) {
            Text("Choose another folder")
        }
        state.message?.let { Text(it, color = MaterialTheme.colorScheme.error) }
        if (state.tracks.isEmpty()) {
            Text("No tracks found in this folder.")
        } else {
            LazyColumn(modifier = Modifier.fillMaxSize()) {
                items(state.tracks, key = { track -> track.uri }) { track ->
                    ListItem(
                        headlineContent = { Text(track.title) },
                        supportingContent = { Text(track.details()) },
                        trailingContent = { Text(formatDuration(track.durationMs)) },
                        modifier = Modifier
                            .fillMaxWidth()
                            .clickable { playTrack(track) },
                    )
                    HorizontalDivider()
                }
            }
        }
    }
}

private fun LibraryTrack.details(): String =
    listOf(artist, album).filter(String::isNotBlank).joinToString(" • ").ifBlank {
        "Unknown artist"
    }

private fun formatDuration(durationMs: Long): String {
    val totalSeconds = (durationMs.coerceAtLeast(0) / 1_000)
    return "%d:%02d".format(totalSeconds / 60, totalSeconds % 60)
}

private fun Throwable.detail(): String = message ?: javaClass.simpleName
