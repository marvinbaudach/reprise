package de.reprise.spike

import android.app.Application
import android.content.ComponentName
import android.content.Intent
import android.os.Looper
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.android.controller.ServiceController
import org.robolectric.annotation.Config
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import uniffi.reprise_android_ffi.AndroidColorScheme
import uniffi.reprise_android_ffi.AndroidPlaybackSession
import uniffi.reprise_android_ffi.AndroidPlaybackSnapshot
import uniffi.reprise_android_ffi.AndroidPlaybackState
import uniffi.reprise_android_ffi.AndroidRepeatMode

@RunWith(RobolectricTestRunner::class)
@Config(
    sdk = [36],
    qualifiers = "w412dp-h916dp-port",
    application = AnalysisTestApplication::class,
)
class MainActivityAnalysisTest {
    @get:Rule
    val compose = createAndroidComposeRule<MainActivity>()

    private val application: AnalysisTestApplication
        get() = RuntimeEnvironment.getApplication() as AnalysisTestApplication

    @After
    fun releaseService() {
        application.releaseService()
    }

    @Test
    fun theRealActivityStartsOnlyForTheOpenSheetAndClosingItCancels() {
        application.service.publish(analysisSnapshot(1))
        idle()

        assertEquals(emptyList<Long>(), application.backend.startedTrackIds)
        compose.onNodeWithTag("library-mini-player").performClick()
        compose.onNodeWithContentDescription("Collapse Now Playing").assertExists()
        compose.waitForIdle()
        assertEquals(listOf(1L), application.backend.startedTrackIds)

        compose.onNodeWithContentDescription("Collapse Now Playing").performClick()
        compose.waitForIdle()

        assertTrue(application.backend.works.single().cancelled)
    }

    @Test
    fun theRealPlaybackObserverCancelsTheOldTrackAndStartsTheNewOne() {
        application.service.publish(analysisSnapshot(1))
        idle()
        compose.onNodeWithTag("library-mini-player").performClick()
        compose.waitForIdle()

        application.service.publish(analysisSnapshot(2))
        idle()
        compose.waitForIdle()

        assertTrue(application.backend.works.first().cancelled)
        assertEquals(listOf(1L, 2L), application.backend.startedTrackIds)
    }

    @Test
    fun activityRecreationCancelsTheOldSessionBeforeTheReplacementCanStart() {
        application.service.publish(analysisSnapshot(1))
        idle()
        compose.onNodeWithTag("library-mini-player").performClick()
        compose.waitForIdle()
        val oldWork = application.backend.works.single()

        compose.activityRule.scenario.recreate()
        idle()

        assertTrue(oldWork.cancelled)
        application.service.republish()
        idle()
        assertEquals(listOf(1L, 1L), application.backend.startedTrackIds)
    }

    @Test
    fun screenOffBroadcastCancelsThroughTheRealActivityReceiver() {
        application.service.publish(analysisSnapshot(1))
        idle()
        compose.onNodeWithTag("library-mini-player").performClick()
        compose.waitForIdle()

        compose.activity.sendBroadcast(Intent(Intent.ACTION_SCREEN_OFF))
        idle()

        assertTrue(application.backend.works.single().cancelled)
    }

    private fun idle() {
        shadowOf(Looper.getMainLooper()).idle()
        compose.waitForIdle()
    }
}

internal class AnalysisTestApplication : Application(),
    MainActivitySurfaceProvider,
    MainActivityAnalysisProvider {
    val backend = ActivityAnalysisBackend()
    private val renderData = ActivityRenderDataStore()
    private lateinit var serviceController: ServiceController<AnalysisPlaybackService>
    lateinit var service: AnalysisPlaybackService
        private set

    override fun onCreate() {
        super.onCreate()
        serviceController = Robolectric.buildService(AnalysisPlaybackService::class.java).create()
        service = serviceController.get()
        shadowOf(this).setComponentNameAndServiceForBindService(
            ComponentName(this, ReprisePlaybackService::class.java),
            service.onBind(Intent(ReprisePlaybackService.LOCAL_BIND_ACTION)),
        )
    }

    override fun mainActivityAnalysis(): MainActivityAnalysisDependencies =
        MainActivityAnalysisDependencies(
            renderData = renderData,
            coordinator = TrackAnalysisController(backend, renderData),
        )

    override fun mainActivitySurface(): MainActivitySurfaceDependencies {
        val tracks = listOf(analysisTrack(1), analysisTrack(2))
        return MainActivitySurfaceDependencies(
            initialTheme = MobileThemeSelection(
                MobileTheme.NOCTURNE,
                AndroidColorScheme.SYSTEM,
                dynamicAvailable = false,
            ),
            initialVisualizer = MobileVisualizer.COVER,
            initialState = LibraryScreenState.Browse(
                titles = LibraryWindow(2, tracks, false),
                albums = LibraryWindow(0, emptyList(), false),
                artists = LibraryWindow(0, emptyList(), false),
            ),
            artwork = { null },
            playbackControls = DisconnectedPlaybackControls,
            chooseFolder = { _, _ -> },
            rescan = {},
            searchTitles = { _, _ -> LibraryWindow(2, tracks, false) },
            listAlbums = { LibraryWindow(0, emptyList(), false) },
            listArtists = { LibraryWindow(0, emptyList(), false) },
            openAlbum = { error("no album in this fixture") },
            listAlbumTracks = { _, _ -> LibraryWindow(0, emptyList(), false) },
            loadTrack = { id, deliver -> deliver(tracks.firstOrNull { it.id == id }) },
            loadPlaybackSettings = { PlaybackSettingsUiState(false, true, emptyList()) },
            setEqualizerEnabled = { PlaybackSettingsUiState(it, true, emptyList()) },
            replaceEqualizerCurve = { PlaybackSettingsUiState(false, true, emptyList()) },
            setGaplessEnabled = { PlaybackSettingsUiState(false, it, emptyList()) },
            selectTheme = { current, _ -> current },
            selectVisualizer = { it },
            animationsEnabled = { true },
            observeAmbientScheduling = {},
        )
    }

    fun releaseService() {
        serviceController.destroy()
    }
}

internal class ActivityAnalysisBackend : TrackAnalysisBackend {
    val startedTrackIds = mutableListOf<Long>()
    val works = mutableListOf<ActivityAnalysisWork>()

    override fun start(
        trackId: Long,
        contentUri: String,
        deliver: (TrackAnalysisResult) -> Unit,
    ): TrackAnalysisWork {
        startedTrackIds += trackId
        return ActivityAnalysisWork().also(works::add)
    }
}

internal class ActivityAnalysisWork : TrackAnalysisWork {
    var cancelled = false
        private set

    override fun cancel() {
        cancelled = true
    }
}

private class ActivityRenderDataStore : TrackRenderDataPort, TrackAnalysisRenderDataStore {
    override val revision = 0
    override fun bars(trackId: Long, barCount: Int): List<TrackRenderBar>? = null
    override fun spectrumColumn(trackId: Long, position: Float): List<Int>? = null
    override fun hasData(trackId: Long, deliver: (Result<Boolean>) -> Unit) =
        deliver(Result.success(false))
    override fun analysisStored(trackId: Long) = Unit
}

internal class AnalysisPlaybackService : ReprisePlaybackService() {
    private var latest: AndroidPlaybackSnapshot? = null
    override fun openCoreSession(port: Media3PlaybackPort): AndroidPlaybackSession? = null

    fun publish(snapshot: AndroidPlaybackSnapshot) {
        latest = snapshot
        coreListener.onPlaybackChanged(snapshot)
    }

    fun republish() {
        latest?.let(coreListener::onPlaybackChanged)
    }
}

private fun analysisTrack(id: Long) = LibraryTrack(
    id = id,
    uri = "content://provider/document/$id.flac",
    title = "Analysis Song $id",
    artist = "Artist",
    album = "Album",
    durationMs = 240_000,
    playCount = 0,
    rating = 0,
)

private fun analysisSnapshot(trackId: Long) = AndroidPlaybackSnapshot(
    state = AndroidPlaybackState.PLAYING,
    currentIndex = 0u,
    currentTrackId = trackId,
    currentTrackUri = "content://provider/document/$trackId.flac",
    positionMs = 0,
    durationMs = 240_000,
    shuffled = false,
    repeat = AndroidRepeatMode.OFF,
    error = null,
)
