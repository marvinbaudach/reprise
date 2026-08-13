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
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollToIndex
import androidx.compose.ui.test.performTextInput
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.swipeLeft
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

    @Test
    fun artistSearchFiltersAlbumsAndClosingItRestoresTheArtistCatalog() {
        compose.onNodeWithText("Artists").performClick()
        compose.onNodeWithTag("library-summary-search").performClick()
        compose.onNodeWithText("Search albums and artists").performTextInput("full")
        compose.waitForIdle()

        compose.onNodeWithText("Full Album 2").assertIsDisplayed()
        compose.onNodeWithText("Artist 2").assertDoesNotExist()

        // The open field owns the way out: clear the query, then close it. The
        // summary row's magnifier is gone for as long as the field is up.
        compose.onNodeWithTag("library-summary-search").assertDoesNotExist()
        compose.onNodeWithContentDescription("Clear search").performClick()
        compose.waitForIdle()
        compose.onNodeWithContentDescription("Close search").performClick()
        compose.waitForIdle()

        compose.onNodeWithText("Artist 2").assertIsDisplayed()
        compose.onNodeWithText("Full Album 2").assertDoesNotExist()
    }

    @Test
    fun filteredAlbumPaginationAndItsSearchSurviveRecreation() {
        compose.onNodeWithText("Artists").performClick()
        compose.onNodeWithTag("library-summary-search").performClick()
        compose.onNodeWithText("Search albums and artists").performTextInput("full")
        compose.waitForIdle()
        compose.onNodeWithTag("library-artist-search-albums-list").performScrollToIndex(200)
        compose.waitForIdle()
        compose.onNodeWithTag("library-artist-search-albums-list").performScrollToIndex(210)
        compose.onNodeWithText("Full Album 212").assertIsDisplayed()

        recreateAt("w916dp-h412dp-land")

        compose.onNodeWithText("Search albums and artists").assertTextContains("full")
        compose.onNodeWithText("Full Album 212").assertIsDisplayed()
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

    /**
     * An open album is a place, and it is the one a turn used to cost most: not
     * a different row but the whole surface, back out to the album list.
     */
    @Test
    fun anOpenAlbumAndTheDepthPagedIntoItBothSurviveTheTurn() {
        openDeepAlbum()
        compose.waitForIdle()
        compose.onNodeWithTag("library-album-tracks-list").performScrollToIndex(200)
        compose.waitForIdle()
        compose.onNodeWithTag("library-album-tracks-list").performScrollToIndex(210)
        compose.waitForIdle()
        compose.onNodeWithText("Album Song 211").assertIsDisplayed()

        recreateAt("w916dp-h412dp-land")

        // Still inside the album, and still where it was left.
        compose.onNodeWithContentDescription("Back to albums").assertIsDisplayed()
        compose.onNodeWithText("Album Song 211").assertIsDisplayed()
        compose.onNodeWithText("Album Song 1").assertDoesNotExist()
    }

    @Test
    fun unheartingDoesNotDiscardAnotherLoadedWindowOrAnOpenAlbumOnRecreate() {
        application.trackRatings[1L] = 5
        application.catalogSize += 1
        recreateAt("w412dp-h916dp-port")

        compose.onNodeWithText("Artists").performClick()
        compose.onNodeWithTag("library-artists-list").performScrollToIndex(200)
        compose.waitForIdle()
        compose.onNodeWithTag("library-artists-list").performScrollToIndex(210)
        compose.waitForIdle()
        compose.onNodeWithText("Artist 211").assertIsDisplayed()

        compose.onNodeWithText("Titles").performClick()
        compose.onNode(
            hasTestTag(TRACK_HEART_TAG) and
                hasAnyAncestor(hasTestTag("library-track-row-1")) and
                hasAnyAncestor(hasTestTag("library-page-TITLES")),
        ).performClick()
        compose.waitForIdle()
        assertTrue(application.controls.ratingRequests.contains(1L to 0))
        compose.onNode(
            hasTestTag(TRACK_HEART_TAG) and
                hasContentDescription("Add to favourites") and
                hasAnyAncestor(hasTestTag("library-track-row-1")),
        ).assertIsDisplayed()

        openDeepAlbum()
        compose.waitForIdle()
        compose.onNodeWithTag("library-album-tracks-list").performScrollToIndex(200)
        compose.waitForIdle()
        compose.onNodeWithTag("library-album-tracks-list").performScrollToIndex(210)
        compose.waitForIdle()
        compose.onNodeWithText("Album Song 211").assertIsDisplayed()

        recreateAt("w916dp-h412dp-land")

        compose.onNodeWithContentDescription("Back to albums").assertIsDisplayed()
        compose.onNodeWithText("Album Song 211").assertIsDisplayed()
        compose.onNodeWithContentDescription("Back to albums").performClick()
        compose.onNodeWithContentDescription("Back to artists").performClick()
        compose.onNodeWithTag("library-artists-list").performScrollToIndex(210)
        compose.onNodeWithText("Artist 211").assertIsDisplayed()
    }

    /**
     * And the same limit as everywhere else: rows kept across the turn can go
     * out of date, so a scan that changed the catalog closes the album rather
     * than showing tracks that may no longer be in it.
     */
    @Test
    fun anAlbumWhoseCatalogChangedUnderTheScreenIsLetGoRatherThanShownStale() {
        openDeepAlbum()
        compose.waitForIdle()
        compose.onNodeWithText("Album Song 1").assertIsDisplayed()

        application.catalogSize = CATALOG_SIZE + 1
        recreateAt("w916dp-h412dp-land")

        compose.onNodeWithContentDescription("Back to albums").assertDoesNotExist()
        compose.onNodeWithText("Album Song 1").assertDoesNotExist()
        compose.onNodeWithText("Artist 1").assertIsDisplayed()
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
    fun currentTrackImportRunsThroughOnCreateAndRecreateWithoutTouchingItsNeighbour() {
        application.replaceQueue(
            listOf(
                configurationTestTrack(1, "Playing"),
                configurationTestTrack(2, "Neighbour"),
            ),
        )
        service.publish(playingSnapshot(positionMs = 12_000))
        shadowOf(Looper.getMainLooper()).idle()
        compose.waitForIdle()

        assertEquals(listOf(1L), application.analysisImports)

        recreateAt("w916dp-h412dp-land")

        assertTrue(application.analysisImports.size >= 2)
        assertEquals(setOf(1L), application.analysisImports.toSet())
    }

    @Test
    fun portraitMeasurementsStayPutAndWideShortUsesThe17aGeometry() {
        assertEquals(1, shadowOf(application).boundServiceConnections.size)
        service.publish(playingSnapshot(positionMs = 12_000))
        shadowOf(Looper.getMainLooper()).idle()
        compose.waitForIdle()

        compose.onNodeWithTag("library-summary-row").assertHeightIsEqualTo(48.dp)
        compose.onNodeWithTag("library-track-row-1").assertHeightIsEqualTo(72.dp)
        compose.onNodeWithTag("library-mini-player").assertHeightIsEqualTo(72.dp)
        compose.onNodeWithTag("library-mini-player").performClick()
        compose.onNodeWithTag("now-playing-cover").assertWidthIsEqualTo(364.dp)
        compose.onNodeWithTag("now-playing-play").assertWidthIsEqualTo(80.dp)
        compose.activity.onBackPressedDispatcher.onBackPressed()
        compose.waitForIdle()

        recreateAt("w916dp-h412dp-land")

        val rail = compose.onNodeWithTag("library-navigation-rail")
        val miniPlayer = compose.onNodeWithTag("library-mini-player")
        rail.assertWidthIsEqualTo(80.dp)
        compose.onNodeWithTag("library-summary-row").assertHeightIsEqualTo(48.dp)
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
            compose.onNodeWithTag("library-summary-row").getUnclippedBoundsInRoot().right - 12.dp,
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

    private fun openDeepAlbum() {
        compose.onNodeWithText("Artists").performClick()
        compose.onNodeWithTag("library-artists-list").performScrollToIndex(0)
        compose.onAllNodesWithText("Artist 1")[0].performClick()
        compose.onNodeWithText(DEEP_ALBUM).performClick()
    }

    private fun androidx.compose.ui.test.SemanticsNodeInteraction.progress(): Float =
        fetchSemanticsNode().config
            .getOrNull(SemanticsProperties.ProgressBarRangeInfo)
            ?.current
            ?: error("No progress semantics")
}

internal open class ConfigurationTestApplication : Application(), MainActivitySurfaceProvider {
    // A write the fake accepts is a write the fake keeps: a rating that vanished
    // the moment it was acknowledged would make every reload path look correct
    // by having nothing to reload.
    val controls = ConfigurationTestPlaybackControls(
        store = { trackId, rating -> trackRatings[trackId] = rating },
        loadUpcoming = ::upcomingWindow,
        playUpcoming = ::playUpcoming,
        moveUpcoming = ::moveUpcoming,
        removeUpcoming = ::removeUpcoming,
        startSleepTimer = { selection -> service.startSleepTimer(selection) },
        cancelSleepTimer = { service.cancelSleepTimer() },
    )
    val analysisImports = mutableListOf<Long>()
    private val trackAnalysis = object : TrackAnalysisPort {
        override val revision = 0L
        override fun prepare(trackId: Long) {
            analysisImports += trackId
        }

        override fun loadBars(
            trackId: Long,
            count: Int,
            deliver: (List<SpectralBar>?) -> Unit,
        ) = deliver(null)
    }
    val ambientScheduleEvents = mutableListOf<Boolean>()
    var animationsEnabled = true
    val trackRatings = mutableMapOf<Long, Int>()
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
            configurationTestTrack(
                id = index.toLong(),
                title = if (index <= 4) "Rotation Song $index" else "Title $index",
                rating = trackRatings[index.toLong()] ?: 2,
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

    /** A paged album catalog whose first row opens a deep detail list. */
    private val albums: List<LibraryAlbum>
        get() = listOf(
            LibraryAlbum(
                title = DEEP_ALBUM,
                artist = "Slowdive",
                representativeUri = "content://provider/album/deep.flac",
                trackCount = catalogSize.toLong(),
                year = 1999,
                totalDurationMs = catalogSize * 120_000L,
            ),
        ) + (2..catalogSize).map { index ->
            LibraryAlbum(
                title = "Full Album $index",
                artist = "Artist $index",
                representativeUri = "content://provider/album/$index.flac",
                trackCount = 1,
                year = 2000 + index % 25,
                totalDurationMs = 120_000,
            )
        }
    private val albumTracks: List<LibraryTrack>
        get() = (1..catalogSize).map { index ->
            configurationTestTrack(
                id = ALBUM_TRACK_ID_BASE + index,
                title = "Album Song $index",
            )
        }

    private val artistTracks: List<LibraryTrack>
        get() = listOf(
            configurationTestTrack(ARTIST_TRACK_ID_BASE + 1, "Artist One · First Album").copy(
                artist = "Artist 1",
                album = "First Album",
            ),
            configurationTestTrack(ARTIST_TRACK_ID_BASE + 2, "Artist One · Second Album").copy(
                artist = "Artist 1",
                album = "Second Album",
            ),
        )
    private val artistAlbums: List<LibraryAlbum>
        get() = listOf(
            LibraryAlbum(
                title = DEEP_ALBUM,
                artist = "Artist 1",
                representativeUri = "content://provider/album/deep.flac",
                trackCount = catalogSize.toLong(),
                year = 2026,
                totalDurationMs = catalogSize * 120_000L,
            ),
        ) + artistTracks.mapIndexed { index, track ->
            LibraryAlbum(
                title = track.album,
                artist = track.artist,
                representativeUri = track.uri,
                trackCount = 1,
                year = 2025 - index,
                totalDurationMs = track.durationMs,
            )
        }

    private fun tracksFor(album: LibraryAlbum): List<LibraryTrack> =
        if (album.title == DEEP_ALBUM) {
            albumTracks
        } else if (album.artist == "Artist 1") {
            artistTracks.filter { track -> track.album == album.title }
        } else {
            albumTracks
        }
    var rememberedDestination = BrowseTab.TITLES
    val rememberedDestinationWrites = mutableListOf<BrowseTab>()
    val artistWindowRequests = mutableListOf<LibraryWindowRange>()
    var currentQueue: List<LibraryTrack> = emptyList()
        private set
    var currentQueueIndex: Int? = null
        private set

    fun replaceQueue(tracks: List<LibraryTrack>, startIndex: Int = 0) {
        currentQueue = tracks
        currentQueueIndex = startIndex
    }

    fun removeUpcomingBehindScreen(trackId: Long) {
        currentQueue = currentQueue.filterNot { it.id == trackId }
    }

    private fun upcomingWindow(range: LibraryWindowRange): LibraryWindow<LibraryTrack> {
        val upcoming = currentQueue.drop((currentQueueIndex ?: -1) + 1)
        return upcoming.window(range)
    }

    private fun playUpcoming(position: Int, expectedTrackId: Long): Boolean {
        val current = currentQueueIndex ?: return false
        val absolute = current + 1 + position
        if (currentQueue.getOrNull(absolute)?.id != expectedTrackId) return false
        val order = currentQueue.toMutableList()
        val promoted = order.removeAt(absolute)
        order.add(current + 1, promoted)
        currentQueue = order
        currentQueueIndex = current + 1
        return true
    }

    private fun moveUpcoming(from: Int, expectedTrackId: Long, to: Int): Boolean {
        val first = (currentQueueIndex ?: return false) + 1
        val source = first + from
        val target = first + to
        if (currentQueue.getOrNull(source)?.id != expectedTrackId || target !in currentQueue.indices) {
            return false
        }
        val order = currentQueue.toMutableList()
        val moved = order.removeAt(source)
        order.add(target, moved)
        currentQueue = order
        return true
    }

    private fun removeUpcoming(position: Int, expectedTrackId: Long): Boolean {
        val absolute = (currentQueueIndex ?: return false) + 1 + position
        if (currentQueue.getOrNull(absolute)?.id != expectedTrackId) return false
        currentQueue = currentQueue.toMutableList().also { it.removeAt(absolute) }
        return true
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
            artists = artists.window(firstLibraryWindow()),
            albumCount = albums.size.toLong(),
            loadedTabs = setOf(BrowseTab.TITLES),
        )
        return MainActivitySurfaceDependencies(
            initialTheme = MobileThemeSelection(
                palette = MobileTheme.NOCTURNE,
                colorScheme = AndroidColorScheme.SYSTEM,
                dynamicAvailable = false,
            ),
            initialState = browse,
            initialBrowseTab = rememberedDestination,
            rememberBrowseTab = { tab ->
                rememberedDestination = tab
                rememberedDestinationWrites += tab
            },
            artwork = { null },
            playbackControls = controls,
            trackAnalysis = trackAnalysis,
            chooseFolder = { _, _ -> },
            rescan = {},
            searchTitles = { query, range ->
                tracks.filter { track -> track.title.contains(query, ignoreCase = true) }
                    .window(range)
            },
            searchAlbums = { query, range ->
                albums.filter { album ->
                    album.title.contains(query, ignoreCase = true) ||
                        album.artist.contains(query, ignoreCase = true)
                }.window(range)
            },
            listArtists = { range ->
                artistWindowRequests += range
                artists.window(range)
            },
            searchArtists = { query, range ->
                artists.filter { artist -> artist.name.contains(query, ignoreCase = true) }
                    .window(range)
            },
            openAlbum = { album -> AlbumTrackList(album, tracksFor(album).window(firstLibraryWindow())) },
            listAlbumTracks = { album, range -> tracksFor(album).window(range) },
            openArtist = { artist ->
                ArtistTrackList(
                    artist = artist,
                    tracks = artistTracks.window(firstLibraryWindow()),
                    albums = artistAlbums.window(firstLibraryWindow()),
                )
            },
            listArtistTracks = { _, range -> artistTracks.window(range) },
            listArtistAlbums = { _, range -> artistAlbums.window(range) },
            listArtistUntaggedTracks = { _, _ -> LibraryWindow.empty() },
            loadTrack = { id, deliver ->
                deliver((tracks + albumTracks + artistTracks).firstOrNull {
                    it.id == id
                })
            },
            playTracks = { selection, _ ->
                replaceQueue(selection.tracks, selection.startIndex)
            },
            loadPlaybackSettings = { PlaybackSettingsUiState(false, true, emptyList()) },
            setEqualizerEnabled = { enabled ->
                PlaybackSettingsUiState(enabled, true, emptyList())
            },
            replaceEqualizerCurve = { PlaybackSettingsUiState(false, true, emptyList()) },
            setGaplessEnabled = { enabled ->
                PlaybackSettingsUiState(false, enabled, emptyList())
            },
            selectTheme = { current, _ -> current },
            animationsEnabled = { animationsEnabled },
            observeAmbientScheduling = ambientScheduleEvents::add,
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

/** Enough rows that the screen has to ask for a second window to reach the end. */
private const val CATALOG_SIZE = 450
private const val DEEP_ALBUM = "Deep Album"

/** Album tracks are their own rows, so they get their own ids and titles. */
private const val ALBUM_TRACK_ID_BASE = 1_000_000L
private const val ARTIST_TRACK_ID_BASE = 2_000_000L

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

internal fun configurationTestTrack(id: Long, title: String, rating: Int = 2) = LibraryTrack(
    id = id,
    uri = "content://provider/document/$id.flac",
    title = title,
    artist = "Artist $id",
    album = "Album",
    durationMs = 120_000,
    playCount = 0,
    rating = rating,
)

private fun playingSnapshot(positionMs: Long) = AndroidPlaybackSnapshot(
    state = AndroidPlaybackState.PLAYING,
    currentIndex = 0u,
    currentTrackId = 1,
    currentTrackUri = "content://provider/document/1.flac",
    positionMs = positionMs,
    durationMs = 120_000,
    automaticAdvanceCount = 0u,
    shuffled = false,
    repeat = AndroidRepeatMode.OFF,
    error = null,
)
