package de.reprise.spike

import android.app.Application
import android.content.ComponentName
import android.content.Intent
import android.os.Looper
import androidx.compose.ui.test.assertIsOff
import androidx.compose.ui.test.assertIsOn
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.android.controller.ServiceController
import org.robolectric.annotation.Config
import uniffi.reprise_android_ffi.AndroidColorScheme
import uniffi.reprise_android_ffi.AndroidStoredLibraryRating

/**
 * Fresh-install and persistence claims cross [MainActivity.onCreate]. The
 * replacement activity reads the controller again; no composable is mounted by hand.
 */
@RunWith(RobolectricTestRunner::class)
@Config(
    sdk = [36],
    qualifiers = "w412dp-h916dp-port",
    application = LibraryRatingTestApplication::class,
)
class MainActivityLibraryRatingSettingTest {
    @get:Rule
    val compose = createAndroidComposeRule<MainActivity>()

    private val application: LibraryRatingTestApplication
        get() = RuntimeEnvironment.getApplication() as LibraryRatingTestApplication

    @After
    fun releaseTheService() {
        application.releaseService()
    }

    @Test
    fun unsetDrawsNoStarsAndAnExplicitChoiceSurvivesRecreateAndFreshRead() {
        compose.onNodeWithText("4/5").assertDoesNotExist()
        assertEquals(1, application.ratingPort.reads)

        openAppearanceSettings()
        compose.onNodeWithTag("settings-library-rating").assertIsOff().performClick()
        compose.onNodeWithTag("settings-library-rating").assertIsOn()
        assertEquals(listOf(true), application.ratingPort.writes)

        compose.activityRule.scenario.recreate()
        shadowOf(Looper.getMainLooper()).idle()
        compose.waitForIdle()

        compose.onNodeWithTag("settings-library-rating").assertIsOn()
        assertEquals(2, application.ratingPort.reads)

        compose.onNodeWithContentDescription("Back to Settings").performClick()
        compose.onNodeWithContentDescription("Back to Library").performClick()
        compose.onNodeWithText("4/5").assertExists()

        assertTrue(LibraryRatingSettingController(application.ratingPort).load())
        assertEquals(3, application.ratingPort.reads)
    }

    private fun openAppearanceSettings() {
        compose.onNodeWithContentDescription("Library actions").performClick()
        compose.onNodeWithText("Settings").performClick()
        compose.onNodeWithContentDescription("Open Appearance").performClick()
    }
}

internal class LibraryRatingTestApplication : Application(),
    MainActivitySurfaceProvider,
    MainActivityLibraryRatingProvider {
    val ratingPort = MutableLibraryRatingPort()
    private lateinit var serviceController: ServiceController<ConfigurationTestPlaybackService>

    override fun onCreate() {
        super.onCreate()
        serviceController = Robolectric.buildService(ConfigurationTestPlaybackService::class.java)
            .create()
        val service = serviceController.get()
        shadowOf(this).setComponentNameAndServiceForBindService(
            ComponentName(this, ReprisePlaybackService::class.java),
            service.onBind(Intent(ReprisePlaybackService.LOCAL_BIND_ACTION)),
        )
    }

    fun releaseService() {
        serviceController.destroy()
    }

    override fun mainActivityLibraryRating(): LibraryRatingSurfaceDependencies {
        val controller = LibraryRatingSettingController(ratingPort)
        return LibraryRatingSurfaceDependencies(
            initialEnabled = controller.load(),
            select = controller::select,
        )
    }

    override fun mainActivitySurface(): MainActivitySurfaceDependencies =
        MainActivitySurfaceDependencies(
            initialTheme = MobileThemeSelection(
                palette = MobileTheme.NOCTURNE,
                colorScheme = AndroidColorScheme.SYSTEM,
                dynamicAvailable = false,
            ),
            initialVisualizer = MobileVisualizer.COVER,
            initialState = LibraryScreenState.Browse(
                titles = LibraryWindow(total = 1, rows = listOf(ratedTrack), hasMore = false),
                albums = LibraryWindow.empty(),
                artists = LibraryWindow.empty(),
            ),
            artwork = { null },
            playbackControls = ConfigurationTestPlaybackControls(),
            chooseFolder = { _, _ -> },
            rescan = {},
            searchTitles = { _, _ ->
                LibraryWindow(total = 1, rows = listOf(ratedTrack), hasMore = false)
            },
            listAlbums = { LibraryWindow.empty() },
            listArtists = { LibraryWindow.empty() },
            openAlbum = { error("No album exists in this fixture") },
            listAlbumTracks = { _, _ -> LibraryWindow.empty() },
            loadTrack = { _, deliver -> deliver(null) },
            loadPlaybackSettings = { PlaybackSettingsUiState(false, true, emptyList()) },
            setEqualizerEnabled = { PlaybackSettingsUiState(it, true, emptyList()) },
            replaceEqualizerCurve = { PlaybackSettingsUiState(false, true, emptyList()) },
            setGaplessEnabled = { PlaybackSettingsUiState(false, it, emptyList()) },
            selectTheme = { current, _ -> current },
            selectVisualizer = { it },
            animationsEnabled = { false },
            observeAmbientScheduling = {},
        )

    private companion object {
        val ratedTrack = LibraryTrack(
            id = 91,
            uri = "content://provider/document/rated.flac",
            title = "Rated Song",
            artist = "Artist",
            album = "Album",
            durationMs = 120_000,
            playCount = 3,
            rating = 4,
        )
    }
}

internal class MutableLibraryRatingPort : LibraryRatingSettingPort {
    var stored: AndroidStoredLibraryRating = AndroidStoredLibraryRating.Unset
    var reads = 0
        private set
    val writes = mutableListOf<Boolean>()

    override fun libraryRatingSetting(): AndroidStoredLibraryRating {
        reads += 1
        return stored
    }

    override fun setLibraryRating(enabled: Boolean) {
        writes += enabled
        stored = if (enabled) AndroidStoredLibraryRating.On else AndroidStoredLibraryRating.Off
    }
}
