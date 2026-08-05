package de.reprise.spike

import android.app.Application
import android.content.ComponentName
import android.content.Intent
import android.os.Looper
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.semantics.ProgressBarRangeInfo
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertHeightIsEqualTo
import androidx.compose.ui.test.assertTextContains
import androidx.compose.ui.test.assertWidthIsEqualTo
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollToIndex
import androidx.compose.ui.test.performTextInput
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.unit.dp
import androidx.lifecycle.ViewModelProvider
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
import uniffi.reprise_android_ffi.AndroidPlaybackSession
import uniffi.reprise_android_ffi.AndroidPlaybackSnapshot
import uniffi.reprise_android_ffi.AndroidPlaybackState
import uniffi.reprise_android_ffi.AndroidRepeatMode

/**
 * Configuration claims run through the path a device uses: MainActivity's
 * `onCreate`, `onStart`, real bind, `onServiceConnected`, Compose surface, and
 * ActivityScenario recreation. The fake surface port replaces only JNI; the
 * ViewModel, window-size decision, layouts, and activity wiring are production.
 */
@RunWith(RobolectricTestRunner::class)
@Config(
    sdk = [36],
    qualifiers = "w412dp-h916dp-port",
    application = ConfigurationTestApplication::class,
)
class MainActivityConfigurationTest {
    @get:Rule
    val compose = createAndroidComposeRule<MainActivity>()

    private val service: ConfigurationTestPlaybackService
        get() = application.service
    private val application: ConfigurationTestApplication
        get() = RuntimeEnvironment.getApplication() as ConfigurationTestApplication

    @After
    fun releaseTheService() {
        application.releaseService()
    }

    @Test
    fun tabSearchAndListAnchorSurviveTheActivityRecreationPath() {
        assertEquals(1, shadowOf(application).boundServiceConnections.size)
        compose.onNodeWithText("Artists").performClick()
        compose.onNodeWithTag("library-artists-list").performScrollToIndex(12)
        compose.waitForIdle()

        val beforeTurn = ViewModelProvider(compose.activity)[MobileSurfaceViewModel::class.java]
        assertEquals(BrowseTab.ARTISTS, beforeTurn.selectedTab)
        assertTrue(beforeTurn.scrollPosition(LibraryListKey.ARTISTS).firstVisibleItemIndex >= 12)

        recreateAt("w916dp-h412dp-land")

        compose.onNodeWithTag("library-navigation-rail").assertIsDisplayed()
        compose.onNodeWithText("Artist 13").assertIsDisplayed()

        compose.onNodeWithText("Titles").performClick()
        compose.onNodeWithContentDescription("Search library").performClick()
        compose.onNodeWithText("Search titles").performTextInput("rotation")
        compose.waitForIdle()

        recreateAt("w412dp-h916dp-port")

        compose.onNodeWithTag("library-navigation-bar").assertIsDisplayed()
        compose.onNodeWithText("Search titles").assertTextContains("rotation")
        compose.onNodeWithText("Rotation Song 1").assertIsDisplayed()
    }

    /**
     * The one the device would have shown and the suite could not: scroll past
     * the first window, turn, and land where you were.
     *
     * `MainActivity.onCreate` restores exactly one window. Everything past it
     * was paged in by the screen, and the anchor is an index into all of it —
     * so an anchor that survives while the rows do not is not a restored
     * position at all: a lazy list asked to start beyond its last item starts
     * at that last item and says nothing.
     */
    @Test
    fun rowsPagedInPastTheFirstWindowSurviveTheTurnAndSoDoesThePlace() {
        assertEquals(1, shadowOf(application).boundServiceConnections.size)
        compose.onNodeWithText("Artists").performClick()
        // The continuation sentinel sits after the last loaded row; reaching it
        // is what asks the library for the next window.
        compose.onNodeWithTag("library-artists-list").performScrollToIndex(200)
        compose.waitForIdle()
        compose.onNodeWithTag("library-artists-list").performScrollToIndex(210)
        compose.waitForIdle()
        compose.onNodeWithText("Artist 211").assertIsDisplayed()

        recreateAt("w916dp-h412dp-land")

        compose.onNodeWithText("Artist 211").assertIsDisplayed()
        // The two places a lost second window puts you instead — the last row
        // of the reloaded window, or the top.
        compose.onNodeWithText("Artist 200").assertDoesNotExist()
        compose.onNodeWithText("Artist 1").assertDoesNotExist()
    }

    /**
     * The other side of keeping rows across the turn: they can go out of date.
     *
     * A scan that finishes between the two activities leaves paged-in rows
     * describing a catalog that is gone, so they are dropped — and then the
     * anchor points past what is loaded. That is where the honest answer
     * matters: the top, which reads as a reset, and not the last row of the
     * reloaded window, which reads as a restored place and is not one.
     */
    @Test
    fun aCatalogThatChangedUnderTheScreenReopensAtTheTopAndNotMidWindow() {
        compose.onNodeWithText("Artists").performClick()
        compose.onNodeWithTag("library-artists-list").performScrollToIndex(200)
        compose.waitForIdle()
        compose.onNodeWithTag("library-artists-list").performScrollToIndex(210)
        compose.waitForIdle()
        compose.onNodeWithText("Artist 211").assertIsDisplayed()

        application.catalogSize = CATALOG_SIZE + 1
        recreateAt("w916dp-h412dp-land")

        compose.onNodeWithText("Artist 1").assertIsDisplayed()
        compose.onNodeWithText("Artist 200").assertDoesNotExist()
        compose.onNodeWithText("Artist 211").assertDoesNotExist()
    }

    @Test
    fun inFlightScrubSurvivesTheTurnWithoutCommittingASeek() {
        assertEquals(1, shadowOf(application).boundServiceConnections.size)
        service.publish(playingSnapshot(positionMs = 12_000))
        shadowOf(Looper.getMainLooper()).idle()
        compose.waitForIdle()
        compose.onNodeWithTag("library-mini-player").performClick()

        val slider = compose.onNodeWithTag("now-playing-seek")
        slider.performTouchInput {
            down(Offset(width * 0.2f, centerY))
            moveTo(Offset(width * 0.6f, centerY))
        }
        val draggedPosition = slider.progress()

        service.publish(playingSnapshot(positionMs = 14_000))
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(draggedPosition, slider.progress(), 0.5f)

        recreateAt("w916dp-h412dp-land")

        val restored = compose.onNodeWithTag("now-playing-seek")
        assertEquals(draggedPosition, restored.progress(), 0.5f)
        assertTrue(application.controls.seekPositions.isEmpty())

        restored.performTouchInput {
            down(Offset(width * 0.7f, centerY))
            up()
        }
        assertEquals(1, application.controls.seekPositions.size)
    }

    @Test
    fun portraitMeasurementsStayPutAndWideShortUsesThe17aGeometry() {
        assertEquals(1, shadowOf(application).boundServiceConnections.size)
        service.publish(playingSnapshot(positionMs = 12_000))
        shadowOf(Looper.getMainLooper()).idle()
        compose.waitForIdle()

        compose.onNodeWithTag("library-top-app-bar").assertHeightIsEqualTo(64.dp)
        compose.onNodeWithTag("library-track-row-1").assertHeightIsEqualTo(72.dp)
        compose.onNodeWithTag("library-mini-player").assertHeightIsEqualTo(72.dp)
        compose.onNodeWithTag("library-mini-player").performClick()
        compose.onNodeWithTag("now-playing-cover").assertWidthIsEqualTo(364.dp)
        compose.onNodeWithTag("now-playing-play").assertWidthIsEqualTo(80.dp)
        compose.onNodeWithContentDescription("Collapse Now Playing").performClick()

        recreateAt("w916dp-h412dp-land")

        val rail = compose.onNodeWithTag("library-navigation-rail")
        val miniPlayer = compose.onNodeWithTag("library-mini-player")
        rail.assertWidthIsEqualTo(80.dp)
        compose.onNodeWithTag("library-top-app-bar").assertHeightIsEqualTo(52.dp)
        compose.onNodeWithTag("library-track-row-1").assertHeightIsEqualTo(64.dp)
        compose.onNodeWithTag("library-track-row-2").assertHeightIsEqualTo(64.dp)
        assertEquals(
            compose.onNodeWithTag("library-track-row-1").getUnclippedBoundsInRoot().top,
            compose.onNodeWithTag("library-track-row-2").getUnclippedBoundsInRoot().top,
        )
        assertTrue(
            compose.onNodeWithTag("library-track-row-2").getUnclippedBoundsInRoot().left >
                compose.onNodeWithTag("library-track-row-1").getUnclippedBoundsInRoot().left,
        )
        assertEquals(
            rail.getUnclippedBoundsInRoot().right + 12.dp,
            miniPlayer.getUnclippedBoundsInRoot().left,
        )
        assertEquals(
            compose.onNodeWithTag("library-top-app-bar").getUnclippedBoundsInRoot().right - 12.dp,
            miniPlayer.getUnclippedBoundsInRoot().right,
        )

        miniPlayer.performClick()
        compose.onNodeWithTag("now-playing-cover").assertWidthIsEqualTo(326.dp)
        compose.onNodeWithTag("now-playing-play").assertWidthIsEqualTo(64.dp)
        compose.onNodeWithTag("now-playing-transport").assertIsDisplayed()
    }

    private fun recreateAt(qualifiers: String) {
        RuntimeEnvironment.setQualifiers(qualifiers)
        compose.activityRule.scenario.recreate()
        shadowOf(Looper.getMainLooper()).idle()
        service.republish()
        shadowOf(Looper.getMainLooper()).idle()
        compose.waitForIdle()
    }

    private fun androidx.compose.ui.test.SemanticsNodeInteraction.progress(): Float =
        fetchSemanticsNode().config
            .getOrNull(SemanticsProperties.ProgressBarRangeInfo)
            ?.current
            ?: error("No progress semantics")
}

internal class ConfigurationTestApplication : Application(), MainActivitySurfaceProvider {
    val controls = ConfigurationTestPlaybackControls()
    private lateinit var serviceController: ServiceController<ConfigurationTestPlaybackService>
    lateinit var service: ConfigurationTestPlaybackService
        private set
    // More rows than one window holds, and served one window at a time. A
    // fixture that hands over the whole catalog at once cannot fail the way the
    // device did: it makes the second window — the part `onCreate` never
    // reloads — unreachable, and every test written on it green by omission.
    /** What a scan would change: the test moves it to act as one. */
    var catalogSize = CATALOG_SIZE
    private val tracks: List<LibraryTrack>
        get() = (1..catalogSize).map { index ->
            configurationTrack(
                id = index.toLong(),
                title = if (index <= 4) "Rotation Song $index" else "Title $index",
            )
        }
    private val artists: List<LibraryArtist>
        get() = (1..catalogSize).map { index ->
            LibraryArtist(
                name = "Artist $index",
                trackCount = index.toLong(),
                albumCount = 1,
                representativeUri = "content://provider/artist/$index.flac",
            )
        }

    override fun onCreate() {
        super.onCreate()
        serviceController = Robolectric.buildService(ConfigurationTestPlaybackService::class.java)
            .create()
        service = serviceController.get()
        shadowOf(this).setComponentNameAndServiceForBindService(
            ComponentName(this, ReprisePlaybackService::class.java),
            service.onBind(Intent(ReprisePlaybackService.LOCAL_BIND_ACTION)),
        )
    }

    fun releaseService() {
        serviceController.destroy()
    }

    override fun mainActivitySurface(): MainActivitySurfaceDependencies {
        val browse = LibraryScreenState.Browse(
            titles = tracks.window(firstLibraryWindow()),
            albums = LibraryWindow.empty(),
            artists = artists.window(firstLibraryWindow()),
        )
        return MainActivitySurfaceDependencies(
            initialTheme = MobileThemeSelection(
                palette = MobileTheme.NOCTURNE,
                colorScheme = AndroidColorScheme.SYSTEM,
                dynamicAvailable = false,
            ),
            initialState = browse,
            artwork = { null },
            playbackControls = controls,
            chooseFolder = { _, _ -> },
            rescan = {},
            searchTitles = { query, range ->
                tracks.filter { track -> track.title.contains(query, ignoreCase = true) }
                    .window(range)
            },
            listAlbums = { browse.albums },
            listArtists = { range -> artists.window(range) },
            openAlbum = { error("Album navigation is outside this test") },
            listAlbumTracks = { _, _ -> LibraryWindow.empty() },
            loadTrack = { id, deliver -> deliver(tracks.firstOrNull { it.id == id }) },
            loadPlaybackSettings = { PlaybackSettingsUiState(false, true, emptyList()) },
            setEqualizerEnabled = { enabled ->
                PlaybackSettingsUiState(enabled, true, emptyList())
            },
            replaceEqualizerCurve = { PlaybackSettingsUiState(false, true, emptyList()) },
            setGaplessEnabled = { enabled ->
                PlaybackSettingsUiState(false, enabled, emptyList())
            },
            selectTheme = { current, _ -> current },
        )
    }
}

internal class ConfigurationTestPlaybackService : ReprisePlaybackService() {
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

class ConfigurationTestPlaybackControls : PlaybackControls {
    val seekPositions = mutableListOf<Long>()

    override fun togglePause() = Unit
    override fun next() = Unit
    override fun previous() = Unit
    override fun seekTo(positionMs: Long) {
        seekPositions += positionMs
    }
    override fun setShuffle(enabled: Boolean) = Unit
    override fun setRepeat(mode: AndroidRepeatMode) = Unit
    override fun setRating(trackId: Long, rating: Int, report: (String?) -> Unit) = report(null)
}

/** Enough rows that the screen has to ask for a second window to reach the end. */
private const val CATALOG_SIZE = 450

/** The library's own paging contract: honour the offset, the limit, and the end. */
private fun <T> List<T>.window(range: LibraryWindowRange): LibraryWindow<T> {
    val from = range.offset.toInt().coerceIn(0, size)
    val until = (from + range.limit.toInt()).coerceIn(from, size)
    return LibraryWindow(
        total = size.toLong(),
        rows = subList(from, until).toList(),
        hasMore = until < size,
    )
}

private fun configurationTrack(id: Long, title: String) = LibraryTrack(
    id = id,
    uri = "content://provider/document/$id.flac",
    title = title,
    artist = "Artist $id",
    album = "Album",
    durationMs = 120_000,
    playCount = 0,
    rating = 2,
)

private fun playingSnapshot(positionMs: Long) = AndroidPlaybackSnapshot(
    state = AndroidPlaybackState.PLAYING,
    currentIndex = 0u,
    currentTrackId = 1,
    currentTrackUri = "content://provider/document/1.flac",
    positionMs = positionMs,
    durationMs = 120_000,
    shuffled = false,
    repeat = AndroidRepeatMode.OFF,
    error = null,
)
