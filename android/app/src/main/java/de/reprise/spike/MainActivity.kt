package de.reprise.spike

import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.ServiceConnection
import android.net.Uri
import android.os.Bundle
import android.os.IBinder
import android.util.Log
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
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
private const val PREFERENCES_NAME = "reprise_android"

class MainActivity : ComponentActivity() {
    private val libraryDelegate = lazy { MusicLibrary.open(filesDir.absolutePath) }
    private val library by libraryDelegate
    private val session by lazy {
        LibrarySession(
            AndroidLibrarySessionPort(
                resolver = contentResolver,
                preferences = getSharedPreferences(PREFERENCES_NAME, MODE_PRIVATE),
                library = library,
            ),
        )
    }
    private var playbackService: ReprisePlaybackService? = null
    private var playbackBound = false
    private val playbackState = mutableStateOf(PlaybackUiState())
    private val playbackConnection = object : ServiceConnection {
        override fun onServiceConnected(name: ComponentName, binder: IBinder) {
            val service = (binder as ReprisePlaybackService.LocalBinder).service()
            playbackService = service
            service.attachObserver { snapshot ->
                runOnUiThread { playbackState.value = snapshot.toUiState() }
            }
        }

        override fun onServiceDisconnected(name: ComponentName) {
            playbackService = null
            playbackState.value = PlaybackUiState()
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val initialState = restoreLibrary()
        setContent {
            MaterialTheme {
                Surface(modifier = Modifier.fillMaxSize()) {
                    LibraryScreen(
                        initialState = initialState,
                        playback = playbackState.value,
                        chooseFolder = ::chooseTree,
                        rescan = ::rescan,
                        playTracks = ::playTracks,
                        togglePause = { runPlaybackCommand("change playback state") { togglePause() } },
                        next = { runPlaybackCommand("skip to the next track") { next() } },
                        previous = { runPlaybackCommand("return to the previous track") { previous() } },
                    )
                }
            }
        }
    }

    override fun onStart() {
        super.onStart()
        val intent = Intent(this, ReprisePlaybackService::class.java).apply {
            action = ReprisePlaybackService.LOCAL_BIND_ACTION
        }
        playbackBound = bindService(intent, playbackConnection, Context.BIND_AUTO_CREATE)
    }

    override fun onStop() {
        playbackService?.detachObserver()
        playbackService = null
        if (playbackBound) {
            unbindService(playbackConnection)
            playbackBound = false
        }
        super.onStop()
    }

    override fun onDestroy() {
        if (libraryDelegate.isInitialized()) {
            libraryDelegate.value.close()
        }
        super.onDestroy()
    }

    private fun restoreLibrary(): LibraryScreenState = runCatching {
        session.restore()
    }.getOrElse { error ->
        val message = "Could not load the saved library: ${error.detail()}"
        Log.e(TAG, message, error)
        runCatching { session.stateAfterFailure(message) }
            .getOrDefault(LibraryScreenState.NoFolder(message))
    }

    private fun chooseTree(treeUri: Uri, report: (LibraryScreenState) -> Unit) {
        runLibraryAction(report) { progress ->
            session.chooseTree(treeUri.toString(), progress)
        }
    }

    private fun rescan(report: (LibraryScreenState) -> Unit) {
        runLibraryAction(report, session::rescan)
    }

    private fun runLibraryAction(
        report: (LibraryScreenState) -> Unit,
        action: ((LibraryScreenState.Scanning) -> Unit) -> LibraryScreenState,
    ) {
        Thread {
            val outcome = runCatching {
                action { progress ->
                    runOnUiThread { report(progress) }
                }
            }
            val state = outcome.getOrElse { error ->
                val message = "Could not update the library: ${error.detail()}"
                Log.e(TAG, message, error)
                session.stateAfterFailure(message)
            }
            runOnUiThread { report(state) }
        }.start()
    }

    private fun playTracks(
        tracks: List<LibraryTrack>,
        startIndex: Int,
        reportError: (String) -> Unit,
    ) {
        runPlaybackCommand("play ${tracks[startIndex].title}", reportError) {
            playTracks(tracks.map(LibraryTrack::uri), startIndex)
        }
    }

    private fun runPlaybackCommand(
        action: String,
        reportError: (String) -> Unit = { message ->
            playbackState.value = playbackState.value.copy(error = message)
        },
        command: ReprisePlaybackService.() -> Unit,
    ) {
        val service = playbackService
        if (service == null) {
            reportError("Could not $action: playback is still connecting.")
            return
        }
        runCatching { service.command() }
            .onFailure { error -> reportError("Could not $action: ${error.detail()}") }
    }
}

internal class UiProgress(
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
    initialState: LibraryScreenState,
    playback: PlaybackUiState,
    chooseFolder: (Uri, (LibraryScreenState) -> Unit) -> Unit,
    rescan: ((LibraryScreenState) -> Unit) -> Unit,
    playTracks: (List<LibraryTrack>, Int, (String) -> Unit) -> Unit,
    togglePause: () -> Unit,
    next: () -> Unit,
    previous: () -> Unit,
) {
    var state by remember { mutableStateOf(initialState) }
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
        LibraryScreenState.TreeUnreadable -> TreeUnreadableScreen(
            chooseFolder = { folderPicker.launch(null) },
        )
        is LibraryScreenState.Scanning -> ScanningScreen(current)
        is LibraryScreenState.TrackList -> TrackListScreen(
            state = current,
            playback = playback,
            chooseFolder = { folderPicker.launch(null) },
            rescan = { rescan { state = it } },
            playTrack = { index ->
                state = current.copy(message = null)
                playTracks(current.tracks, index) { message ->
                    val visible = state
                    if (visible is LibraryScreenState.TrackList) {
                        state = visible.copy(message = message)
                    }
                }
            },
            togglePause = togglePause,
            next = next,
            previous = previous,
        )
    }
}

@Composable
private fun TreeUnreadableScreen(chooseFolder: () -> Unit) {
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
    playback: PlaybackUiState,
    chooseFolder: () -> Unit,
    rescan: () -> Unit,
    playTrack: (Int) -> Unit,
    togglePause: () -> Unit,
    next: () -> Unit,
    previous: () -> Unit,
) {
    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = 16.dp, vertical = 20.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            Button(onClick = rescan) {
                Text("Rescan")
            }
            Button(onClick = chooseFolder) {
                Text("Choose another folder")
            }
        }
        state.message?.let { Text(it, color = MaterialTheme.colorScheme.error) }
        PlaybackControls(
            state = playback,
            togglePause = togglePause,
            next = next,
            previous = previous,
        )
        if (state.tracks.isEmpty() && state.message == null) {
            Text("No tracks found in this folder.")
        } else {
            LazyColumn(modifier = Modifier.fillMaxSize()) {
                itemsIndexed(state.tracks, key = { _, track -> track.uri }) { index, track ->
                    ListItem(
                        headlineContent = { Text(track.title) },
                        supportingContent = { Text(track.details()) },
                        trailingContent = { Text(formatDuration(track.durationMs)) },
                        modifier = Modifier
                            .fillMaxWidth()
                            .clickable { playTrack(index) },
                    )
                    HorizontalDivider()
                }
            }
        }
    }
}

@Composable
private fun PlaybackControls(
    state: PlaybackUiState,
    togglePause: () -> Unit,
    next: () -> Unit,
    previous: () -> Unit,
) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Button(onClick = previous, enabled = state.ready && state.currentIndex != null) {
            Text("Previous")
        }
        Button(onClick = togglePause, enabled = state.ready && state.currentIndex != null) {
            Text(state.playPauseLabel)
        }
        Button(onClick = next, enabled = state.ready && state.currentIndex != null) {
            Text("Next")
        }
        Text(state.positionReadout)
    }
    state.error?.let { Text(it, color = MaterialTheme.colorScheme.error) }
}

private fun LibraryTrack.details(): String =
    listOf(artist, album).filter(String::isNotBlank).joinToString(" • ").ifBlank {
        "Unknown artist"
    }

internal fun formatDuration(durationMs: Long): String {
    val totalSeconds = (durationMs.coerceAtLeast(0) / 1_000)
    return "%d:%02d".format(totalSeconds / 60, totalSeconds % 60)
}

private fun Throwable.detail(): String = message ?: javaClass.simpleName
