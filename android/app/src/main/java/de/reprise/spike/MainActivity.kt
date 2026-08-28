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
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.viewModels
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
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.material3.windowsizeclass.ExperimentalMaterial3WindowSizeClassApi
import androidx.compose.material3.windowsizeclass.calculateWindowSizeClass
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
import de.reprise.spike.ui.theme.RepriseTheme
import de.reprise.spike.ui.theme.AmbientTrueBlack
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.launch
import uniffi.reprise_android_ffi.AndroidColorScheme
import uniffi.reprise_android_ffi.AndroidEqualizerPoint
import uniffi.reprise_android_ffi.AndroidEqualizerPreset
import uniffi.reprise_android_ffi.AndroidStoredLibraryDestination
import uniffi.reprise_android_ffi.MusicLibrary
import uniffi.reprise_android_ffi.ScanProgressListener
import uniffi.reprise_android_ffi.ScanProgressUpdate
import uniffi.reprise_android_ffi.TrashAction
import uniffi.reprise_android_ffi.standardEqualizerPresets

private const val TAG = "RepriseScan"
private const val PREFERENCES_NAME = "reprise_android"
private const val NOTIFICATION_PERMISSION_ASKED = "notification_permission_asked"
internal const val PLAYBACK_BIND_WATCHDOG_MS = 2_000L
internal const val PLAYBACK_BIND_FAILURE_LOG = "Playback service bind did not connect"
class MainActivity : ComponentActivity() {
    // The core is told where to cache covers instead of assuming an XDG
    // directory that does not exist here.
    private val surfaceState by viewModels<MobileSurfaceViewModel>()
    private val library by lazy {
        surfaceState.retainLibrary {
            MusicLibrary.open(filesDir.absolutePath, cacheDir.absolutePath)
        }
    }
    private var usesProductionSurface = false
    private val equalizerPresets by lazy(::equalizerPresetUi)
    private val sessionPort by lazy {
        AndroidLibrarySessionPort(
            resolver = contentResolver,
            preferences = getSharedPreferences(PREFERENCES_NAME, MODE_PRIVATE),
            library = library,
            afterScan = surfaceState::startArtistPhotoBackfill,
        )
    }
    private val artistPortraitPrefetchDelegate = lazy {
        ArtistPortraitPrefetch(sessionPort)
    }
    private val artistPortraitPrefetch by artistPortraitPrefetchDelegate
    private val session by lazy {
        LibrarySession(
            port = sessionPort,
            startPortraitPrefetch = artistPortraitPrefetch::start,
            scanMonitor = surfaceState.libraryScanMonitor,
        )
    }
    private val artworkDelegate = lazy {
        TrackArtwork(
            resolve = session::artworkFor,
            resolveArtistPortraitCached = session::artistPortraitCached,
            resolveArtistPortraitFetched = session::artistPortraitFetched,
        )
    }
    private val artwork by artworkDelegate

    private val libraryWrites = LibraryWrites(
        onMainThread = { work -> runOnUiThread { work() } },
    )

    /**
     * Heart taps use the shared write lane and answer back on the main thread.
     * The lambda defers touching [session] until a rating is actually made, so
     * opening the library still happens when the screen asks for it and not
     * before.
     */
    private val ratings = RatingWriter(
        write = { trackId, favourite -> session.setFavourite(trackId, favourite) },
        libraryWrites = libraryWrites,
    )

    /**
     * The playing track's row, read off the main thread and answered back on
     * it. The database read stays off the main thread even though `track_by_id`
     * no longer waits for a folder scan, and the same deferred lambda means the
     * library still opens when the screen asks for it.
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
    private val visualizerPreference = AndroidVisualizerPreference(
        libraryWrites = libraryWrites,
        library = { library },
    )
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
        connectedService = { boundService.value },
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
    private val boundService = MutableStateFlow<ReprisePlaybackService?>(null)
    private var playbackBound = false
    private var playbackBindWatchdog: Job? = null
    private val playbackState = mutableStateOf(PlaybackUiState())
    internal val currentPlaybackState: PlaybackUiState
        get() = playbackState.value
    private val playbackSettingsRevision = mutableStateOf(0L)
    private val visualSceneEngineFactory = mutableStateOf<VisualSceneEngineFactory>(
        NativeVisualSceneEngineFactory,
    )
    internal val currentVisualSceneEngineFactory: VisualSceneEngineFactory
        get() = visualSceneEngineFactory.value
    private val playbackConnection = object : ServiceConnection {
        override fun onServiceConnected(name: ComponentName, binder: IBinder) {
            val service = (binder as ReprisePlaybackService.LocalBinder).service()
            playbackBindWatchdog?.cancel()
            playbackBindWatchdog = null
            boundService.value = service
            visualSceneEngineFactory.value = service.visualSceneEngineFactory()
        }

        override fun onServiceDisconnected(name: ComponentName) {
            boundService.value = null
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
        val surface = surfaceProvider?.mainActivitySurface() ?: run {
            usesProductionSurface = true
            surfaceState.connectArtistPhotoBackfill(library) { work -> runOnUiThread(work) }
            productionSurface().also { surfaceState.startArtistPhotoBackfill() }
        }
        collectPlaybackServiceState()
        setContent {
            var themeSelection by remember { mutableStateOf(surface.initialTheme) }
            var onlineSourcesEnabled by remember {
                mutableStateOf(surface.onlineSourcesEnabled())
            }
            val darkPalette = themeSelection.usesDarkPalette(isSystemInDarkTheme())
            val libraryPlayback by remember { derivedStateOf { playbackState.value.libraryPlayback() } }
            val playbackProgress = remember { { playbackState.value.progressFraction } }
            val nowPlayingPlayback = remember { { playbackState.value } }
            surfaceState.initializeSelectedTab(
                surface.initialStoredDestination.toBrowseTab(),
                surface.rememberBrowseTab,
            )
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
                            LocalAmbientMotionController provides ambientMotion,
                            LocalVisualizerPreference provides visualizerPreference,
                            LocalVisualSceneEngineFactory provides visualSceneEngineFactory.value,
                            LocalLibraryPerformanceObserver provides surface.libraryPerformanceObserver,
                        ) {
                            LibraryScreen(
                                initialState = surface.initialState,
                                playback = libraryPlayback,
                                playbackProgress = playbackProgress,
                                nowPlayingPlayback = nowPlayingPlayback,
                                playbackSettingsRevision = playbackSettingsRevision.value,
                                surfaceLayout = surfaceLayout,
                                surfaceState = surfaceState,
                                chooseFolder = surface.chooseFolder,
                                rescan = surface.rescan,
                                searchTitles = surface.searchTitles,
                                searchAlbums = surface.searchAlbums,
                                listArtists = surface.listArtists,
                                searchArtists = surface.searchArtists,
                                openAlbum = surface.openAlbum,
                                listAlbumTracks = surface.listAlbumTracks,
                                openArtist = surface.openArtist,
                                listArtistTracks = surface.listArtistTracks,
                                listArtistAlbums = surface.listArtistAlbums,
                                listArtistUntaggedTracks = surface.listArtistUntaggedTracks,
                                loadTrack = surface.loadTrack,
                                playTracks = surface.playTracks,
                                loadPlaybackSettings = surface.loadPlaybackSettings,
                                setEqualizerEnabled = surface.setEqualizerEnabled,
                                replaceEqualizerCurve = surface.replaceEqualizerCurve,
                                setGaplessEnabled = surface.setGaplessEnabled,
                                onlineSourcesEnabled = onlineSourcesEnabled,
                                setOnlineSourcesEnabled = { enabled ->
                                    libraryWrites.submitAnswered(
                                        work = {
                                            surface.setOnlineSourcesEnabled(enabled).getOrThrow()
                                        },
                                        report = { outcome ->
                                            outcome.onSuccess {
                                                onlineSourcesEnabled = enabled
                                                if (enabled) {
                                                    surfaceState.startArtistPhotoBackfill()
                                                } else {
                                                    surfaceState.cancelArtistPhotoBackfill()
                                                }
                                            }.onFailure { error ->
                                                Log.e(
                                                    TAG,
                                                    "Could not change online source settings",
                                                    error,
                                                )
                                            }
                                        },
                                    )
                                },
                                themeSelection = themeSelection,
                                selectTheme = { palette ->
                                    val currentSelection = themeSelection
                                    libraryWrites.submitAnswered(
                                        work = { surface.selectTheme(currentSelection, palette) },
                                        report = { outcome ->
                                            outcome.onSuccess { themeSelection = it }
                                                .onFailure { error ->
                                                    Log.e(TAG, "Could not change theme", error)
                                                }
                                        },
                                    )
                                },
                            )
                        }
                    }
                }
            }
        }
    }

    private fun collectPlaybackServiceState() {
        lifecycleScope.launch {
            repeatOnLifecycle(Lifecycle.State.STARTED) {
                launch {
                    boundService.flatMapLatest { service ->
                        service?.playbackSnapshots ?: flowOf(null)
                    }.collect { snapshot ->
                        if (snapshot != null) {
                            playbackState.value = snapshot.toUiState().copy(
                                sleepTimer = playbackState.value.sleepTimer,
                            )
                        }
                    }
                }
                launch {
                    boundService.flatMapLatest { service ->
                        service?.settingsRevisions ?: flowOf<Long?>(null)
                    }.collect { revision ->
                        if (revision != null) playbackSettingsRevision.value += 1L
                    }
                }
                launch {
                    boundService.flatMapLatest { service ->
                        service?.sleepTimerStates ?: flowOf<SleepTimerUiState?>(null)
                    }.collect { timer ->
                        if (timer != null) {
                            playbackState.value = playbackState.value.copy(sleepTimer = timer)
                        }
                    }
                }
            }
        }
    }

    private fun productionSurface(): MainActivitySurfaceDependencies {
        val initialStoredDestination = restoreStoredDestination()
        val initialBrowseTab = initialStoredDestination.toBrowseTab()
        return MainActivitySurfaceDependencies(
            initialTheme = restoreTheme(),
            initialState = restoreLibrary(initialBrowseTab),
            initialStoredDestination = initialStoredDestination,
            rememberBrowseTab = ::rememberBrowseTab,
            artwork = { artwork },
            playbackControls = playbackControls,
            trackAnalysis = analysis,
            chooseFolder = ::chooseTree,
            rescan = ::rescan,
            searchTitles = { query, range -> session.searchTitles(query, range) },
            searchAlbums = { query, range -> session.searchAlbums(query, range) },
            listArtists = { range -> session.listArtists(range) },
            searchArtists = { query, range -> session.searchArtists(query, range) },
            openAlbum = { album -> session.openAlbum(album) },
            listAlbumTracks = { album, range -> session.listAlbumTracks(album, range) },
            openArtist = { artist -> session.openArtist(artist) },
            listArtistTracks = { artist, range -> session.listArtistTracks(artist, range) },
            listArtistAlbums = { artist, range -> session.listArtistAlbums(artist, range) },
            listArtistUntaggedTracks = { artist, range ->
                session.listArtistUntaggedTracks(artist, range)
            },
            loadTrack = ::loadTrack,
            playTracks = ::playTracks,
            loadPlaybackSettings = ::loadPlaybackSettings,
            setEqualizerEnabled = ::setEqualizerEnabled,
            replaceEqualizerCurve = ::replaceEqualizerCurve,
            setGaplessEnabled = ::setGaplessEnabled,
            selectTheme = { current, palette -> themeController.select(current, palette) },
            onlineSourcesEnabled = {
                runCatching { library.onlineSourcesEnabled() }
                    .onFailure { error ->
                        Log.e(TAG, "Could not load online source settings", error)
                    }
                    .getOrDefault(false)
            },
            setOnlineSourcesEnabled = { enabled ->
                runCatching { library.setOnlineSourcesEnabled(enabled) }
            },
            animationsEnabled = ValueAnimator::areAnimatorsEnabled,
            observeAmbientScheduling = {},
        )
    }

    private fun restoreStoredDestination(): AndroidStoredLibraryDestination = runCatching {
        library.libraryDestinationSetting()
    }.getOrElse { error ->
        Log.e(TAG, "Could not load the library destination; using Titles", error)
        AndroidStoredLibraryDestination.Titles
    }

    private fun rememberBrowseTab(tab: BrowseTab) {
        val destination = tab.toLibraryDestinationChoice() ?: return
        libraryWrites.submitUnanswered(
            work = { library.setLibraryDestination(destination) },
            onFailure = { error ->
                Log.e(TAG, "Could not remember the library destination", error)
            },
        )
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
        playbackBindWatchdog?.cancel()
        val intent = Intent(this, ReprisePlaybackService::class.java).apply {
            action = ReprisePlaybackService.LOCAL_BIND_ACTION
        }
        playbackBound = bindService(intent, playbackConnection, Context.BIND_AUTO_CREATE)
        if (!playbackBound) {
            Log.w(TAG, "$PLAYBACK_BIND_FAILURE_LOG: bindService returned false")
            return
        }
        playbackBindWatchdog = lifecycleScope.launch {
            delay(PLAYBACK_BIND_WATCHDOG_MS)
            if (boundService.value != null || !lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED)) {
                return@launch
            }
            Log.w(
                TAG,
                "$PLAYBACK_BIND_FAILURE_LOG within $PLAYBACK_BIND_WATCHDOG_MS ms; retrying once",
            )
            val retryAccepted = bindService(intent, playbackConnection, Context.BIND_AUTO_CREATE)
            playbackBound = playbackBound || retryAccepted
            playbackBindWatchdog = null
        }
    }

    override fun onResume() {
        super.onResume()
        if (!usesProductionSurface) return
        Thread {
            runCatching { session.autoScan() }
                .onSuccess { state ->
                    state?.let { runOnUiThread { surfaceState.updateLibraryState(it) } }
                }
                .onFailure { error -> Log.w(TAG, "Silent library scan failed", error) }
        }.start()
    }

    override fun onPause() {
        super.onPause()
    }

    override fun onStop() {
        playbackBindWatchdog?.cancel()
        playbackBindWatchdog = null
        boundService.value = null
        visualSceneEngineFactory.value = NativeVisualSceneEngineFactory
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
        // First, and before final ViewModel cleanup can close the retained
        // library handle: the caller waits briefly for answered work, then the
        // stopped lane continues it because a control still waits for its result.
        // Unanswered-only preferences are dropped at once so a rotation never
        // waits behind the scan-held writer.
        if (!libraryWrites.shutdown()) {
            Log.w(TAG, "A library setting was still being written when the screen closed")
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
        if (artistPortraitPrefetchDelegate.isInitialized()) {
            artistPortraitPrefetchDelegate.value.shutdown()
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
        val snapshot = boundService.value?.equalizerSnapshot()
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
            equalizerCurve = stored.equalizerCurve.map { point ->
                EqualizerCurvePoint(point.frequencyHz, point.gainDb)
            },
            equalizerPresets = equalizerPresets,
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
        boundService.value?.reloadPlaybackSettings()
        return loadPlaybackSettings()
    }

    private fun replaceEqualizerCurve(
        points: List<EqualizerCurvePoint>,
    ): PlaybackSettingsUiState {
        library.replaceEqualizerCurve(
            points.map { point -> AndroidEqualizerPoint(point.frequencyHz, point.gainDb) },
        )
        boundService.value?.reloadPlaybackSettings()
        return loadPlaybackSettings()
    }

    private fun setGaplessEnabled(enabled: Boolean): PlaybackSettingsUiState {
        library.setGaplessEnabled(enabled)
        boundService.value?.reloadPlaybackSettings()
        return loadPlaybackSettings()
    }

    private fun runPlaybackCommand(
        action: String,
        reportError: (String) -> Unit = { message ->
            playbackState.value = playbackState.value.copy(error = message)
        },
        command: ReprisePlaybackService.() -> Unit,
    ) {
        val service = boundService.value
        if (service == null) {
            reportError("Could not $action: playback is still connecting.")
            return
        }
        runCatching { service.command() }
            .onFailure { error -> reportError("Could not $action: ${error.detail()}") }
    }

}

internal fun equalizerPresetUi(): List<EqualizerPresetUi> =
    standardEqualizerPresets().map { definition ->
        EqualizerPresetUi(
            name = definition.preset.displayName(),
            curve = definition.curve.map { point ->
                EqualizerCurvePoint(point.frequencyHz, point.gainDb)
            },
        )
    }

internal fun AndroidEqualizerPreset.displayName(): String = when (this) {
    AndroidEqualizerPreset.FLAT -> "Flat"
    AndroidEqualizerPreset.ROCK -> "Rock"
    AndroidEqualizerPreset.POP -> "Pop"
    AndroidEqualizerPreset.BASS -> "Bass"
    AndroidEqualizerPreset.CLASSICAL -> "Classical"
    AndroidEqualizerPreset.JAZZ -> "Jazz"
    AndroidEqualizerPreset.ELECTRONIC -> "Electronic"
    AndroidEqualizerPreset.VOCAL -> "Vocal & Podcast"
    AndroidEqualizerPreset.HEADPHONES -> "Headphones"
    AndroidEqualizerPreset.LATE_NIGHT -> "Late Night"
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

private fun Throwable.detail(): String = message ?: javaClass.simpleName
