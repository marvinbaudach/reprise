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
import androidx.activity.SystemBarStyle
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.material3.Button
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import de.reprise.spike.ui.theme.RepriseTheme
import uniffi.reprise_android_ffi.MusicLibrary
import uniffi.reprise_android_ffi.ScanProgressListener
import uniffi.reprise_android_ffi.ScanProgressUpdate

private const val TAG = "RepriseScan"
private const val PREFERENCES_NAME = "reprise_android"

/** No scrim behind the system bars: the app's own ground is what shows through. */
private const val TRANSPARENT_SYSTEM_BAR = 0

class MainActivity : ComponentActivity() {
    // The core is told where to cache covers instead of assuming an XDG
    // directory that does not exist here.
    private val libraryDelegate = lazy {
        MusicLibrary.open(filesDir.absolutePath, cacheDir.absolutePath)
    }
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
    private val artworkDelegate = lazy { TrackArtwork(resolve = session::artworkFor) }
    private val artwork by artworkDelegate
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
        // From SDK 35 the system draws behind the bars whether an app asks or
        // not; saying so explicitly is what lets us pick light bar icons for a
        // ground that is dark even when the system is not.
        enableEdgeToEdge(
            statusBarStyle = SystemBarStyle.dark(TRANSPARENT_SYSTEM_BAR),
            navigationBarStyle = SystemBarStyle.dark(TRANSPARENT_SYSTEM_BAR),
        )
        super.onCreate(savedInstanceState)
        val initialState = restoreLibrary()
        setContent {
            RepriseTheme {
                Surface(
                    modifier = Modifier.fillMaxSize(),
                    color = MaterialTheme.colorScheme.background,
                ) {
                    // The frame starts below the clock. Material 3's
                    // NavigationBar consumes its own bottom inset, so the root
                    // must not consume that inset a second time.
                    Box(modifier = Modifier.statusBarsPadding()) {
                        CompositionLocalProvider(LocalTrackArtwork provides artwork) {
                            LibraryScreen(
                                initialState = initialState,
                                playback = playbackState.value,
                                chooseFolder = ::chooseTree,
                                rescan = ::rescan,
                                searchTitles = session::searchTitles,
                                listAlbums = session::listAlbums,
                                listArtists = session::listArtists,
                                openAlbum = session::openAlbum,
                                listAlbumTracks = session::listAlbumTracks,
                                playTracks = ::playTracks,
                                togglePause = {
                                    runPlaybackCommand("change playback state") { togglePause() }
                                },
                                next = {
                                    runPlaybackCommand("skip to the next track") { next() }
                                },
                                previous = {
                                    runPlaybackCommand("return to the previous track") { previous() }
                                },
                            )
                        }
                    }
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
        if (artworkDelegate.isInitialized()) {
            artworkDelegate.value.shutdown()
        }
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
        selection: PlaybackSelection,
        reportError: (String) -> Unit,
    ) {
        val selected = selection.tracks[selection.startIndex]
        runPlaybackCommand("play ${selected.title}", reportError) {
            playTracks(selection.tracks.map(LibraryTrack::uri), selection.startIndex)
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
    searchTitles: (String, LibraryWindowRange) -> LibraryWindow<LibraryTrack>,
    listAlbums: (LibraryWindowRange) -> LibraryWindow<LibraryAlbum>,
    listArtists: (LibraryWindowRange) -> LibraryWindow<LibraryArtist>,
    openAlbum: (LibraryAlbum) -> AlbumTrackList,
    listAlbumTracks: (LibraryAlbum, LibraryWindowRange) -> LibraryWindow<LibraryTrack>,
    playTracks: (PlaybackSelection, (String) -> Unit) -> Unit,
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
        is LibraryScreenState.Browse -> BrowseScreen(
            state = current,
            playback = playback,
            chooseFolder = { folderPicker.launch(null) },
            rescan = { rescan { state = it } },
            searchTitles = searchTitles,
            listAlbums = listAlbums,
            listArtists = listArtists,
            openAlbum = openAlbum,
            listAlbumTracks = listAlbumTracks,
            playTracks = playTracks,
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

private fun Throwable.detail(): String = message ?: javaClass.simpleName
