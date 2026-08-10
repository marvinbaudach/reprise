package de.reprise.spike

import android.Manifest
import android.animation.ValueAnimator
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.ServiceConnection
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.IBinder
import android.provider.DocumentsContract
import android.util.Log
import android.view.View
import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.activity.SystemBarStyle
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import androidx.compose.material3.windowsizeclass.ExperimentalMaterial3WindowSizeClassApi
import androidx.compose.material3.windowsizeclass.calculateWindowSizeClass
import androidx.lifecycle.viewmodel.compose.viewModel
import de.reprise.spike.ui.theme.RepriseTheme
import de.reprise.spike.ui.theme.AmbientTrueBlack
import uniffi.reprise_android_ffi.AndroidColorScheme
import uniffi.reprise_android_ffi.AndroidEqualizerPoint
import uniffi.reprise_android_ffi.MusicLibrary
import uniffi.reprise_android_ffi.ScanProgressListener
import uniffi.reprise_android_ffi.ScanProgressUpdate
import uniffi.reprise_android_ffi.TrashAction

private const val TAG = "RepriseScan"
private const val PREFERENCES_NAME = "reprise_android"
private const val NOTIFICATION_PERMISSION_ASKED = "notification_permission_asked"
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

    /**
     * Heart taps, off the main thread and answered back on it. The lambda defers
     * touching [session] until a rating is actually made, so opening the
     * library still happens when the screen asks for it and not before.
     */
    private val ratings = RatingWriter(
        write = { trackId, favourite -> session.setFavourite(trackId, favourite) },
        onMainThread = { work -> runOnUiThread { work() } },
    )

    /**
     * The playing track's row, read off the main thread and answered back on
     * it. Same reason as [ratings] — `track_by_id` takes the library lock a
     * folder scan holds for its whole walk — and the same deferred lambda, so
     * the library still opens when the screen asks for it.
     */
    private val tracks = TrackLoader(
        read = { trackId -> session.trackById(trackId) },
        onMainThread = { work -> runOnUiThread { work() } },
    )
    private val analysisDelegate = lazy {
        TrackAnalysisLoader(
            importAnalysis = { trackId -> library.importTrackAnalysis(trackId) },
            readBars = { trackId, count ->
                library.trackRenderBars(trackId, count.toUInt())?.map { it.toSpectralBar() }
            },
            readSpectrogram = { trackId -> library.trackSpectrogram(trackId) },
            onMainThread = { work -> runOnUiThread { work() } },
        )
    }
    private val analysis by analysisDelegate
    private val nowPlayingViewSettings by lazy { AndroidNowPlayingViewSettings(library) }
    private val injectedNowPlayingViewSettings by lazy {
        InjectedNowPlayingViewSettings(getSharedPreferences(PREFERENCES_NAME, MODE_PRIVATE))
    }
    private val themeController by lazy {
        ThemeController(
            port = AndroidThemeSettingsPort(library),
            dynamicAvailable = Build.VERSION.SDK_INT >= Build.VERSION_CODES.S,
        )
    }
    private val trashAction = object : TrashAction {
        override fun trash(uri: String): String? = runCatching {
            if (DocumentsContract.deleteDocument(contentResolver, Uri.parse(uri))) {
                null
            } else {
                "The document provider refused to delete this file."
            }
        }.getOrElse { error -> error.detail() }
    }
    /**
     * Every transport command the surface can issue, bound once here instead of
     * threaded through two composables that issue none of them.
     */
    private val playbackControls = ActivityPlaybackControls(
        command = { action, operation -> runPlaybackCommand(action, command = operation) },
        connectedService = { playbackService },
        postToMain = { work -> runOnUiThread(work) },
        setFavouriteAction = ::setFavourite,
        trashAction = trashAction,
        playTrackIdsAction = ::playTrackIds,
    )
    private val notificationPermission = registerForActivityResult(
        ActivityResultContracts.RequestPermission(),
    ) { granted ->
        if (!granted) {
            Log.i(TAG, "Playback runs without a notification: the user said no")
        }
    }
    private var playbackService: ReprisePlaybackService? = null
    private var playbackBound = false
    private val playbackState = mutableStateOf(PlaybackUiState())
    private val playbackSettingsRevision = mutableStateOf(0L)
    private val playbackConnection = object : ServiceConnection {
        override fun onServiceConnected(name: ComponentName, binder: IBinder) {
            val service = (binder as ReprisePlaybackService.LocalBinder).service()
            playbackService = service
            service.attachObserver { snapshot ->
                runOnUiThread {
                    playbackState.value = snapshot.toUiState().copy(
                        sleepTimer = playbackState.value.sleepTimer,
                    )
                }
            }
            service.attachSettingsObserver {
                runOnUiThread {
                    playbackSettingsRevision.value += 1L
                }
            }
            service.attachSleepTimerObserver { timer ->
                runOnUiThread { playbackState.value = playbackState.value.copy(sleepTimer = timer) }
            }
        }

        override fun onServiceDisconnected(name: ComponentName) {
            playbackService = null
            playbackState.value = PlaybackUiState()
            playbackSettingsRevision.value += 1L
        }
    }

    @OptIn(ExperimentalMaterial3WindowSizeClassApi::class)
    override fun onCreate(savedInstanceState: Bundle?) {
        // From SDK 35 the system draws behind the bars whether an app asks or
        // not; saying so explicitly is what lets us pick light bar icons for a
        // ground that is dark even when the system is not.
        configureEdgeToEdge(darkPalette = true)
        super.onCreate(savedInstanceState)
        val surfaceProvider = application as? MainActivitySurfaceProvider
        val surface = surfaceProvider?.mainActivitySurface() ?: productionSurface()
        // Deferred for the same reason as [tracks] and [analysisDelegate]: reading
        // it opens the library, and onCreate must survive a core that will not
        // load so that restoreLibrary can show the failure instead of crashing.
        // The scene asks for it during composition, which is late enough.
        val activeNowPlayingViewSettings by lazy {
            if (surfaceProvider == null) {
                nowPlayingViewSettings
            } else {
                injectedNowPlayingViewSettings
            }
        }
        setContent {
            var themeSelection by remember { mutableStateOf(surface.initialTheme) }
            val darkPalette = themeSelection.usesDarkPalette(isSystemInDarkTheme())
            val surfaceState: MobileSurfaceViewModel = viewModel()
            surfaceState.initializeSelectedTab(surface.initialBrowseTab, surface.rememberBrowseTab)
            val surfaceLayout = surfaceLayoutFor(calculateWindowSizeClass(this))
            val ambientMotion = remember(surface.observeAmbientScheduling) {
                AmbientMotionController(surface.observeAmbientScheduling)
            }
            BindAmbientRuntime(ambientMotion, surface.animationsEnabled)
            LaunchedEffect(darkPalette) { configureEdgeToEdge(darkPalette) }
            LaunchedEffect(surfaceState.dockMode) { setDockWindowMode(surfaceState.dockMode) }
            RepriseTheme(themeSelection, darkPalette) {
                Surface(
                    modifier = Modifier.fillMaxSize(),
                    color = if (surfaceState.dockMode) {
                        AmbientTrueBlack
                    } else {
                        MaterialTheme.colorScheme.background
                    },
                ) {
                    // The frame starts below the clock. Material 3's
                    // NavigationBar consumes its own bottom inset, so the root
                    // must not consume that inset a second time.
                    Box(
                        modifier = if (surfaceState.dockMode) {
                            Modifier
                        } else {
                            Modifier.statusBarsPadding()
                        },
                    ) {
                        CompositionLocalProvider(
                            LocalTrackArtwork provides surface.artwork(),
                            LocalPlaybackControls provides surface.playbackControls,
                            LocalAlbumTrackIds provides { album -> session.albumTrackIds(album) },
                            LocalTrackAnalysis provides surface.trackAnalysis,
                            LocalNowPlayingViewSettings provides activeNowPlayingViewSettings,
                            LocalAmbientMotionController provides ambientMotion,
                        ) {
                            LibraryScreen(
                                initialState = surface.initialState,
                                playback = playbackState.value,
                                playbackSettingsRevision = playbackSettingsRevision.value,
                                surfaceLayout = surfaceLayout,
                                surfaceState = surfaceState,
                                chooseFolder = surface.chooseFolder,
                                rescan = surface.rescan,
                                searchTitles = surface.searchTitles,
                                listAlbums = surface.listAlbums,
                                listArtists = surface.listArtists,
                                openAlbum = surface.openAlbum,
                                listAlbumTracks = surface.listAlbumTracks,
                                openArtist = surface.openArtist,
                                listArtistTracks = surface.listArtistTracks,
                                listFavourites = surface.listFavourites,
                                loadTrack = surface.loadTrack,
                                playTracks = surface.playTracks,
                                loadPlaybackSettings = surface.loadPlaybackSettings,
                                setEqualizerEnabled = surface.setEqualizerEnabled,
                                replaceEqualizerCurve = surface.replaceEqualizerCurve,
                                setGaplessEnabled = surface.setGaplessEnabled,
                                themeSelection = themeSelection,
                                selectTheme = { palette ->
                                    runCatching {
                                        surface.selectTheme(themeSelection, palette)
                                    }.onSuccess { selection ->
                                        themeSelection = selection
                                    }.onFailure { error ->
                                        Log.e(TAG, "Could not change theme", error)
                                    }
                                },
                            )
                        }
                    }
                }
            }
        }
    }

    private fun productionSurface(): MainActivitySurfaceDependencies {
        val initialBrowseTab = restoreBrowseTab()
        return MainActivitySurfaceDependencies(
            initialTheme = restoreTheme(),
            initialState = restoreLibrary(initialBrowseTab),
            initialBrowseTab = initialBrowseTab,
            rememberBrowseTab = ::rememberBrowseTab,
            artwork = { artwork },
            playbackControls = playbackControls,
            trackAnalysis = analysis,
            chooseFolder = ::chooseTree,
            rescan = ::rescan,
            searchTitles = { query, range -> session.searchTitles(query, range) },
            listAlbums = { range -> session.listAlbums(range) },
            listArtists = { range -> session.listArtists(range) },
            openAlbum = { album -> session.openAlbum(album) },
            listAlbumTracks = { album, range -> session.listAlbumTracks(album, range) },
            openArtist = { artist -> session.openArtist(artist) },
            listArtistTracks = { artist, range -> session.listArtistTracks(artist, range) },
            listFavourites = { range -> session.listFavourites(range) },
            loadTrack = ::loadTrack,
            playTracks = ::playTracks,
            loadPlaybackSettings = ::loadPlaybackSettings,
            setEqualizerEnabled = ::setEqualizerEnabled,
            replaceEqualizerCurve = ::replaceEqualizerCurve,
            setGaplessEnabled = ::setGaplessEnabled,
            selectTheme = { current, palette -> themeController.select(current, palette) },
            animationsEnabled = ValueAnimator::areAnimatorsEnabled,
            observeAmbientScheduling = {},
        )
    }

    private fun restoreBrowseTab(): BrowseTab = runCatching {
        library.libraryDestinationSetting().toBrowseTab()
    }.getOrElse { error ->
        Log.e(TAG, "Could not load the library destination; using Titles", error)
        BrowseTab.TITLES
    }

    private fun rememberBrowseTab(tab: BrowseTab) {
        runCatching { library.setLibraryDestination(tab.toLibraryDestinationChoice()) }
            .onFailure { error -> Log.e(TAG, "Could not remember the library destination", error) }
    }

    private fun configureEdgeToEdge(darkPalette: Boolean) {
        val transparent = SystemBarStyle.auto(
            TRANSPARENT_SYSTEM_BAR,
            TRANSPARENT_SYSTEM_BAR,
        ) { darkPalette }
        enableEdgeToEdge(
            statusBarStyle = transparent,
            navigationBarStyle = transparent,
        )
    }

    @Suppress("DEPRECATION")
    private fun setDockWindowMode(docked: Boolean) {
        val keepScreenOn = WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON
        if (docked) {
            window.addFlags(keepScreenOn)
        } else {
            window.clearFlags(keepScreenOn)
        }
        WindowCompat.getInsetsController(window, window.decorView).run {
            systemBarsBehavior =
                WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
            if (docked) {
                hide(WindowInsetsCompat.Type.systemBars())
            } else {
                show(WindowInsetsCompat.Type.systemBars())
            }
        }
        // Robolectric exposes this legacy request while the compat controller
        // above is the API the device follows. Keeping both also covers API 26.
        window.decorView.systemUiVisibility = if (docked) {
            View.SYSTEM_UI_FLAG_FULLSCREEN or
                View.SYSTEM_UI_FLAG_HIDE_NAVIGATION or
                View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY
        } else {
            View.SYSTEM_UI_FLAG_VISIBLE
        }
    }

    private fun restoreTheme(): MobileThemeSelection = runCatching {
        themeController.load()
    }.getOrElse { error ->
        Log.e(TAG, "Could not load appearance settings; using Nocturne", error)
        MobileThemeSelection(
            palette = MobileTheme.NOCTURNE,
            colorScheme = AndroidColorScheme.SYSTEM,
            dynamicAvailable = Build.VERSION.SDK_INT >= Build.VERSION_CODES.S,
        )
    }

    override fun onStart() {
        super.onStart()
        val intent = Intent(this, ReprisePlaybackService::class.java).apply {
            action = ReprisePlaybackService.LOCAL_BIND_ACTION
        }
        playbackBound = bindService(intent, playbackConnection, Context.BIND_AUTO_CREATE)
    }

    override fun onResume() {
        super.onResume()
    }

    override fun onPause() {
        super.onPause()
    }

    override fun onStop() {
        playbackService?.detachObserver()
        playbackService?.detachSettingsObserver()
        playbackService?.detachSleepTimerObserver()
        playbackService = null
        if (playbackBound) {
            unbindService(playbackConnection)
            playbackBound = false
        }
        super.onStop()
    }

    override fun onDestroy() {
        setDockWindowMode(false)
        // Stop accepting boundary calls while letting the single ordered lane
        // finish operations already submitted against the service.
        playbackControls.shutdown()
        // Compose disposal is not the release boundary: Android may destroy
        // the activity while dock mode is still the ViewModel's current mode.
        // First, and before the library handle is closed below: a heart tap that
        // is still queued has to reach the database it was written for, and a
        // write arriving at a closed handle would be a crash rather than a lost
        // rating.
        if (!ratings.shutdown()) {
            Log.w(TAG, "A rating was still being written when the screen closed")
        }
        // Artwork and the playing track's row deliberately get no such drain.
        // Both are reads, so the requests still queued are dropped rather than
        // waited for, and a read already running needs no help: the bindings
        // count a handle's in-flight calls and the close below frees the Rust
        // object only once the last one has returned. `TrackArtwork.shutdown`
        // carries the reasoning.
        tracks.shutdown()
        if (analysisDelegate.isInitialized() && !analysisDelegate.value.shutdown()) {
            Log.w(TAG, "Track analysis was still being prepared when the screen closed")
        }
        if (artworkDelegate.isInitialized()) {
            artworkDelegate.value.shutdown()
        }
        if (libraryDelegate.isInitialized()) {
            libraryDelegate.value.close()
        }
        super.onDestroy()
    }

    private fun restoreLibrary(selectedTab: BrowseTab): LibraryScreenState = runCatching {
        session.restore(selectedTab)
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

    /**
     * Internal rather than private because the test drives it: it is the same
     * call the library screen makes, and reaching it through the activity is
     * the only way to cover [onServiceConnected] and the service start below.
     */
    internal fun playTracks(
        selection: PlaybackSelection,
        reportError: (String) -> Unit,
    ) {
        val selected = selection.tracks[selection.startIndex]
        keepPlaybackRunningWithoutThisScreen()
        askAboutTheNotificationOnce()
        runPlaybackCommand("play ${selected.title}", reportError) {
            playTracks(selection.tracks, selection.startIndex)
        }
    }

    private fun playTrackIds(trackIds: List<Long>, startIndex: Int) {
        keepPlaybackRunningWithoutThisScreen()
        askAboutTheNotificationOnce()
        runPlaybackCommand("play tracks") { playTrackIds(trackIds, startIndex) }
    }

    /**
     * Gives the playback service a lifetime that does not end with this screen.
     *
     * Until this call the service was only ever bound, and a bound service is
     * destroyed the moment its last client unbinds — which a rotation is, since
     * it destroys the activity. The device showed it plainly: mid-track the
     * session was destroyed and audio focus abandoned, so turning the phone
     * stopped the music.
     *
     * Deliberately [startService] and not `startForegroundService`: starting
     * carries no promise to post a notification within five seconds, and there
     * is nothing to show yet at the instant the command is issued. Media3 does
     * the foreground step itself, notification and all, once the player really
     * plays — see [ReprisePlaybackService.onCreate].
     *
     * A start is only legal while the app is in the foreground, which it is:
     * the user just tapped a track. A refusal is logged and left alone, because
     * playback itself still works — it just would not survive this screen.
     */
    private fun keepPlaybackRunningWithoutThisScreen() {
        runCatching { startService(Intent(this, ReprisePlaybackService::class.java)) }
            .onFailure { error ->
                Log.w(TAG, "Playback will not outlive this screen", error)
            }
    }

    /**
     * Asks, once ever, for the permission that lets the playing notification be
     * seen.
     *
     * From API 33 a notification nobody allowed is simply not shown, and the
     * playback notification is where the transport lives while the app is away:
     * lock screen, shade, headphones. So the permission belongs to this app.
     *
     * Asked at the first playback command rather than at launch, because that
     * is the moment the request explains itself — the user just started a song,
     * and the answer buys the controls for it. A cold-start prompt would be a
     * question without an occasion, and Android only grants two refusals before
     * the dialog stops appearing at all.
     *
     * Asked once and then remembered: a prompt that returns after every
     * rotation would be worse than no prompt. A refusal changes nothing about
     * playback — the service still runs, it just plays out of sight.
     */
    private fun askAboutTheNotificationOnce() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) {
            return
        }
        val preferences = getSharedPreferences(PREFERENCES_NAME, MODE_PRIVATE)
        if (preferences.getBoolean(NOTIFICATION_PERMISSION_ASKED, false)) {
            return
        }
        runCatching { notificationPermission.launch(Manifest.permission.POST_NOTIFICATIONS) }
            // Remembered only once the question was really put, so a launcher
            // that refused to fire does not count as an answer.
            .onSuccess {
                preferences.edit().putBoolean(NOTIFICATION_PERMISSION_ASKED, true).apply()
            }
            .onFailure { error -> Log.w(TAG, "Could not ask about notifications", error) }
    }

    /**
     * Asks for the playing track's row. The failure is handled inside
     * [TrackLoader], which is also the only place that can do anything about
     * it: it is the one that can ask again.
     */
    private fun loadTrack(trackId: Long, deliver: (LibraryTrack?) -> Unit) {
        tracks.load(trackId, deliver)
    }

    /**
     * Persists one favourite change off the main thread and answers on it with the
     * failure to show, or null when it was saved.
     *
     * Deliberately not through [playbackState]: that whole record is replaced
     * by the next 500 ms position tick, so a message left there is gone before
     * anyone reads it. The favourite write belongs to the control the user
     * tapped.
     *
     * Which is also why the outcome is logged when it is a failure: the control
     * the user tapped can be gone by the time a queued write answers, and a
     * refusal nobody is left to show belongs in `logcat` rather than nowhere.
     */
    private fun setFavourite(trackId: Long, favourite: Boolean, report: (String?) -> Unit) {
        ratings.setFavourite(trackId, favourite) { outcome ->
            val message = outcome.fold(
                onSuccess = { null },
                onFailure = { error -> "Could not save rating: ${error.detail()}" },
            )
            message?.let { Log.w(TAG, it) }
            report(message)
        }
    }

    private fun loadPlaybackSettings(): PlaybackSettingsUiState {
        val stored = library.playbackSettings()
        val snapshot = playbackService?.equalizerSnapshot()
        val bands = snapshot?.bands.orEmpty().map { band ->
            EqualizerBandUi(
                frequencyHz = band.frequencyHz,
                gainDb = band.gainDb,
                minimumGainDb = band.minimumGainDb,
                maximumGainDb = band.maximumGainDb,
            )
        }
        return PlaybackSettingsUiState(
            equalizerEnabled = stored.equalizerEnabled,
            gaplessEnabled = stored.gaplessEnabled,
            equalizerBands = bands,
            // A snapshot that reports no equalizer is a session we *have* asked:
            // saying "start playback" there would be false while a track plays.
            equalizerBandsAbsence = if (snapshot != null && !snapshot.available) {
                EqualizerBandsAbsence.NO_EQUALIZER_ON_THIS_DEVICE
            } else {
                EqualizerBandsAbsence.NO_PLAYBACK_YET
            },
        )
    }

    private fun setEqualizerEnabled(enabled: Boolean): PlaybackSettingsUiState {
        library.setEqualizerEnabled(enabled)
        playbackService?.reloadPlaybackSettings()
        return loadPlaybackSettings()
    }

    private fun replaceEqualizerCurve(
        points: List<EqualizerCurvePoint>,
    ): PlaybackSettingsUiState {
        library.replaceEqualizerCurve(
            points.map { point -> AndroidEqualizerPoint(point.frequencyHz, point.gainDb) },
        )
        playbackService?.reloadPlaybackSettings()
        return loadPlaybackSettings()
    }

    private fun setGaplessEnabled(enabled: Boolean): PlaybackSettingsUiState {
        library.setGaplessEnabled(enabled)
        playbackService?.reloadPlaybackSettings()
        return loadPlaybackSettings()
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
    playbackSettingsRevision: Long,
    surfaceLayout: SurfaceLayout,
    surfaceState: MobileSurfaceViewModel,
    chooseFolder: (Uri, (LibraryScreenState) -> Unit) -> Unit,
    rescan: ((LibraryScreenState) -> Unit) -> Unit,
    searchTitles: (String, LibraryWindowRange) -> LibraryWindow<LibraryTrack>,
    listAlbums: (LibraryWindowRange) -> LibraryWindow<LibraryAlbum>,
    listArtists: (LibraryWindowRange) -> LibraryWindow<LibraryArtist>,
    openAlbum: (LibraryAlbum) -> AlbumTrackList,
    listAlbumTracks: (LibraryAlbum, LibraryWindowRange) -> LibraryWindow<LibraryTrack>,
    openArtist: (LibraryArtist) -> ArtistTrackList,
    listArtistTracks: (LibraryArtist, LibraryWindowRange) -> LibraryWindow<LibraryTrack>,
    listFavourites: (LibraryWindowRange) -> LibraryWindow<LibraryTrack>,
    loadTrack: (Long, (LibraryTrack?) -> Unit) -> Unit,
    playTracks: (PlaybackSelection, (String) -> Unit) -> Unit,
    loadPlaybackSettings: () -> PlaybackSettingsUiState,
    setEqualizerEnabled: (Boolean) -> PlaybackSettingsUiState,
    replaceEqualizerCurve: (List<EqualizerCurvePoint>) -> PlaybackSettingsUiState,
    setGaplessEnabled: (Boolean) -> PlaybackSettingsUiState,
    themeSelection: MobileThemeSelection,
    selectTheme: (MobileTheme) -> Unit,
) {
    var state by remember { mutableStateOf(initialState) }
    val folderPicker = rememberLauncherForActivityResult(
        contract = ActivityResultContracts.OpenDocumentTree(),
    ) { treeUri ->
        if (treeUri != null) {
            chooseFolder(treeUri) { state = it }
        }
    }
    val launchFolderPicker = { folderPicker.launch(folderPickerInitialUri()) }

    when (val current = state) {
        is LibraryScreenState.NoFolder -> NoFolderScreen(
            message = current.message,
            chooseFolder = launchFolderPicker,
        )
        LibraryScreenState.TreeUnreadable -> TreeUnreadableScreen(
            chooseFolder = launchFolderPicker,
        )
        is LibraryScreenState.Scanning -> ScanningScreen(current)
        is LibraryScreenState.Browse -> BrowseScreen(
            state = current,
            playback = playback,
            playbackSettingsRevision = playbackSettingsRevision,
            surfaceLayout = surfaceLayout,
            surfaceState = surfaceState,
            chooseFolder = launchFolderPicker,
            rescan = { rescan { state = it } },
            searchTitles = searchTitles,
            listAlbums = listAlbums,
            listArtists = listArtists,
            openAlbum = openAlbum,
            listAlbumTracks = listAlbumTracks,
            openArtist = openArtist,
            listArtistTracks = listArtistTracks,
            listFavourites = listFavourites,
            loadTrack = loadTrack,
            playTracks = playTracks,
            loadPlaybackSettings = loadPlaybackSettings,
            setEqualizerEnabled = setEqualizerEnabled,
            replaceEqualizerCurve = replaceEqualizerCurve,
            setGaplessEnabled = setGaplessEnabled,
            themeSelection = themeSelection,
            selectTheme = selectTheme,
        )
    }
}

private fun Throwable.detail(): String = message ?: javaClass.simpleName
