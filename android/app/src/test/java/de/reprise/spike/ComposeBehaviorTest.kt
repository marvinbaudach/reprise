package de.reprise.spike

import androidx.activity.BackEventCompat
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.semantics.ProgressBarRangeInfo
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.SemanticsNodeInteraction
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertContentDescriptionEquals
import androidx.compose.ui.test.assertTextContains
import androidx.compose.ui.test.junit4.StateRestorationTester
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performSemanticsAction
import androidx.compose.ui.test.performTextInput
import androidx.compose.ui.test.performTouchInput
import de.reprise.spike.ui.theme.RepriseTheme
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.reprise_android_ffi.AndroidColorScheme
import uniffi.reprise_android_ffi.AndroidPlaybackState

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36], qualifiers = "w500dp-h1000dp")
class ComposeBehaviorTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    /// Nocturne with dynamic colour off: these tests are about behaviour, and a
    /// wallpaper-seeded palette would make them depend on the host's wallpaper.
    private val nocturneForTests = MobileThemeSelection(
        palette = MobileTheme.NOCTURNE,
        colorScheme = AndroidColorScheme.SYSTEM,
        dynamicAvailable = false,
    )

    @Test
    fun seekDragOwnsTheHeadUntilReleaseThenSnapshotsResume() {
        val controls = RecordingPlaybackControls()
        var playback by mutableStateOf(testPlayback(positionMs = 20_000))
        compose.setContent {
            RepriseTheme(nocturneForTests, darkPalette = true) {
                CompositionLocalProvider(LocalPlaybackControls provides controls) {
                    NowPlayingSheet(
                        track = testTrack(rating = 2),
                        playback = playback,
                        close = {},
                    )
                }
            }
        }
        val slider = compose.onNode(SemanticsMatcher.keyIsDefined(SemanticsProperties.ProgressBarRangeInfo))

        slider.performTouchInput {
            down(Offset(width * 0.25f, centerY))
            (1..5).forEach { step ->
                advanceEventTime(20)
                moveTo(Offset(width * (0.25f + step * 0.1f), centerY))
            }
        }
        val draggedPosition = slider.progress()

        compose.runOnIdle { playback = testPlayback(positionMs = 10_000) }
        assertEquals(draggedPosition, slider.progress(), 0.5f)

        slider.performTouchInput { up() }
        assertTrue(controls.seekPositions.isNotEmpty())
        compose.runOnIdle { playback = testPlayback(positionMs = 30_000) }
        assertEquals(30_000f, slider.progress(), 0.5f)
    }

    @Test
    fun failedFavouriteShowsTheErrorWithoutMovingTheHeart() {
        val failure = "Could not save rating: track is missing."
        val controls = RecordingPlaybackControls(ratingFailure = failure)
        compose.setContent {
            RepriseTheme(nocturneForTests, darkPalette = true) {
                CompositionLocalProvider(LocalPlaybackControls provides controls) {
                    NowPlayingSheet(
                        track = testTrack(rating = 2),
                        playback = testPlayback(positionMs = 20_000),
                        close = {},
                    )
                }
            }
        }

        heart("Add to favourites").performClick()

        compose.onNodeWithText(failure).assertIsDisplayed()
        heart("Add to favourites").assertIsDisplayed()
        assertEquals(listOf(830L to 5), controls.ratingRequests)
    }

    /** The heart changes only after the database accepts the requested write. */
    @Test
    fun theHeartWaitsForTheWriteToAnswerAndOnlyThenMoves() {
        val controls = RecordingPlaybackControls(answerImmediately = false)
        compose.setContent {
            RepriseTheme(nocturneForTests, darkPalette = true) {
                CompositionLocalProvider(LocalPlaybackControls provides controls) {
                    NowPlayingSheet(
                        track = testTrack(rating = 2),
                        playback = testPlayback(positionMs = 20_000),
                        close = {},
                    )
                }
            }
        }

        heart("Add to favourites").performClick()
        compose.waitForIdle()

        assertEquals(listOf(830L to 5), controls.ratingRequests)
        heart("Add to favourites").assertIsDisplayed()

        compose.runOnIdle { controls.answerPending(null) }

        heart("Remove from favourites").assertIsDisplayed()
    }

    @Test
    fun miniPlayerOpensTheSheetBackClosesItAndTheLibraryKeepsWorking() {
        val tracks = listOf(
            testTrack(rating = 2).copy(id = 830, title = "First Song", playCount = 1),
            testTrack(rating = 4).copy(
                id = 831,
                uri = "content://provider/document/second.flac",
                title = "Second Song",
            ),
        )
        val browse = LibraryScreenState.Browse(
            titles = LibraryWindow(total = 2, rows = tracks, hasMore = false),
            artists = LibraryWindow.empty(),
        )
        val playedIndices = mutableListOf<Int>()
        var playback by mutableStateOf(
            testPlayback(positionMs = 20_000).copy(
                currentIndex = null,
                currentTrackId = null,
                currentTrackUri = null,
            ),
        )
        compose.setContent {
            RepriseTheme(nocturneForTests, darkPalette = true) {
                BrowseScreen(
                    state = browse,
                    playback = playback,
                    playbackSettingsRevision = 0,
                    chooseFolder = {},
                    rescan = {},
                    themeSelection = nocturneForTests,
                    selectTheme = {},
                    searchTitles = { _, _ -> browse.titles },
                    searchAlbums = { _, _ -> LibraryWindow.empty() },
                    listArtists = { browse.artists },
                    openAlbum = { error("Album navigation is outside this test") },
                    listAlbumTracks = { _, _ -> LibraryWindow.empty() },
                    loadTrack = { id, deliver -> deliver(tracks.firstOrNull { it.id == id }) },
                    playTracks = { selection, _ ->
                        playedIndices += selection.startIndex
                        val selected = selection.tracks[selection.startIndex]
                        playback = playback.copy(
                            currentIndex = selection.startIndex,
                            currentTrackId = selected.id,
                            currentTrackUri = selected.uri,
                        )
                    },
                    loadPlaybackSettings = {
                        PlaybackSettingsUiState(false, true, emptyList())
                    },
                    setEqualizerEnabled = { enabled ->
                        PlaybackSettingsUiState(enabled, true, emptyList())
                    },
                    replaceEqualizerCurve = {
                        PlaybackSettingsUiState(false, true, emptyList())
                    },
                    setGaplessEnabled = { enabled ->
                        PlaybackSettingsUiState(false, enabled, emptyList())
                    },
                )
            }
        }

        // The row is one clickable node too, so everything described below it
        // is merged into what it announces. The play count earns its place;
        // the cover does not, and the separately clickable heart keeps its own
        // node so rating a track cannot also start it.
        compose.onNodeWithText("First Song")
            .assertContentDescriptionEquals("1 play")
        compose.onNodeWithText("Second Song")
            .assertContentDescriptionEquals("27 plays")

        compose.onNodeWithText("First Song").performClick()
        // Found by the action it offers rather than by a description of its
        // own: a content description on this node would be merged over the
        // track it announces, and announcing the track is the point of it.
        val miniPlayer = compose.onNode(hasClickLabel("Open Now Playing"))
        miniPlayer.assertTextContains("First Song")
        miniPlayer.assertTextContains("Artist")
        miniPlayer.assert(SemanticsMatcher.keyNotDefined(SemanticsProperties.ContentDescription))

        miniPlayer.performClick()
        compose.onNodeWithTag("now-playing-transport").assertIsDisplayed()
        compose.onAllNodesWithText("First Song").assertCountEquals(3)

        // The full-screen sheet should intercept pointer hit testing. Calling
        // the retained row's public action proves its state and wiring stayed
        // alive underneath rather than replacing the Library composition.
        compose.onNodeWithText("Second Song")
            .performSemanticsAction(SemanticsActions.OnClick) { click -> click() }
        compose.onAllNodesWithText("Second Song").assertCountEquals(3)
        assertEquals(listOf(0, 1), playedIndices)

        compose.runOnIdle { compose.activity.onBackPressedDispatcher.onBackPressed() }
        compose.onNodeWithTag("now-playing-transport").assertDoesNotExist()
        compose.onAllNodesWithText("Second Song").assertCountEquals(2)
    }

    /**
     * The mutable snapshot below stands in for the bound playback service: it
     * outlives every activity instance, while the composition and every
     * `remember` inside it are destroyed by [ActivityScenario.recreate]. The
     * replacement activity installs the same production surface its
     * `onCreate` would install, and that surface must recover one complete
     * library row from the session-owned stable id.
     *
     * The final recreation keeps `nowPlayingExpanded` true in saved UI state
     * but removes the session identity. That must render neither sheet nor mini
     * player; a stale retained row is worse than an honest blank surface.
     */
    @Test
    fun playingTrackSurvivesActivityRecreationAndStoppedPlaybackStaysBlank() {
        val track = testTrack(rating = 2).copy(title = "Rotation Song")
        val browse = LibraryScreenState.Browse(
            titles = LibraryWindow(total = 1, rows = listOf(track), hasMore = false),
            artists = LibraryWindow.empty(),
        )
        val loadedIds = mutableListOf<Long>()
        var playback by mutableStateOf(
            testPlayback(positionMs = 20_000).copy(
                currentTrackId = track.id,
                currentTrackUri = track.uri,
            ),
        )
        val content: @Composable () -> Unit = {
            RepriseTheme(nocturneForTests, darkPalette = true) {
                BrowseScreen(
                    state = browse,
                    playback = playback,
                    playbackSettingsRevision = 0,
                    chooseFolder = {},
                    rescan = {},
                    themeSelection = nocturneForTests,
                    selectTheme = {},
                    searchTitles = { _, _ -> browse.titles },
                    searchAlbums = { _, _ -> LibraryWindow.empty() },
                    listArtists = { browse.artists },
                    openAlbum = { error("Album navigation is outside this test") },
                    listAlbumTracks = { _, _ -> LibraryWindow.empty() },
                    loadTrack = { id, deliver ->
                        loadedIds += id
                        deliver(track.takeIf { it.id == id })
                    },
                    playTracks = { _, _ -> },
                    loadPlaybackSettings = {
                        PlaybackSettingsUiState(false, true, emptyList())
                    },
                    setEqualizerEnabled = { enabled ->
                        PlaybackSettingsUiState(enabled, true, emptyList())
                    },
                    replaceEqualizerCurve = {
                        PlaybackSettingsUiState(false, true, emptyList())
                    },
                    setGaplessEnabled = { enabled ->
                        PlaybackSettingsUiState(false, enabled, emptyList())
                    },
                )
            }
        }
        compose.setContent(content)
        compose.onNode(hasClickLabel("Open Now Playing")).assertTextContains("Rotation Song")

        compose.activityRule.scenario.recreate()
        compose.runOnUiThread { compose.activity.setContent(content = content) }
        compose.waitForIdle()

        val restoredMiniPlayer = compose.onNode(hasClickLabel("Open Now Playing"))
        restoredMiniPlayer.assertTextContains("Rotation Song")
        restoredMiniPlayer.performClick()
        compose.onNodeWithTag("now-playing-transport").assertIsDisplayed()

        compose.runOnIdle {
            playback = PlaybackUiState(ready = true)
        }
        compose.activityRule.scenario.recreate()
        compose.runOnUiThread { compose.activity.setContent(content = content) }
        compose.waitForIdle()

        compose.onNode(hasClickLabel("Open Now Playing")).assertDoesNotExist()
        compose.onNodeWithTag("now-playing-transport").assertDoesNotExist()
        assertEquals(listOf(track.id, track.id), loadedIds)
    }

    /**
     * The playing track's row is read off the main thread, so its answer
     * arrives after the composition that asked for it. The last answered row
     * stays visible while the replacement is outstanding, then changes in
     * place when the new answer arrives.
     *
     * Before the first answer there is no row to retain. Once there is one, a
     * track change keeps it in place without making it actionable. A stopped
     * session still shows nothing, whatever arrives afterwards.
     *
     * The fake here never answers by itself, which is the point: a surface that
     * reads the row inside its own composition cannot pass this, because the
     * row only ever exists after the composition that asked for it.
     */
    @Test
    fun theLastAnsweredRowStaysUntilItsReplacementAndStopStillBlanksIt() {
        val first = testTrack(rating = 2).copy(id = 830, title = "First Song")
        val second = testTrack(rating = 4).copy(
            id = 831,
            uri = "content://provider/document/second.flac",
            title = "Second Song",
        )
        val browse = LibraryScreenState.Browse(
            titles = LibraryWindow(total = 2, rows = listOf(first, second), hasMore = false),
            artists = LibraryWindow.empty(),
        )
        val pending = mutableMapOf<Long, (LibraryTrack?) -> Unit>()
        var playback by mutableStateOf(
            testPlayback(positionMs = 20_000).copy(
                currentTrackId = first.id,
                currentTrackUri = first.uri,
            ),
        )
        compose.setContent {
            RepriseTheme(nocturneForTests, darkPalette = true) {
                BrowseScreen(
                    state = browse,
                    playback = playback,
                    playbackSettingsRevision = 0,
                    chooseFolder = {},
                    rescan = {},
                    themeSelection = nocturneForTests,
                    selectTheme = {},
                    searchTitles = { _, _ -> browse.titles },
                    searchAlbums = { _, _ -> LibraryWindow.empty() },
                    listArtists = { browse.artists },
                    openAlbum = { error("Album navigation is outside this test") },
                    listAlbumTracks = { _, _ -> LibraryWindow.empty() },
                    loadTrack = { id, deliver -> pending[id] = deliver },
                    playTracks = { _, _ -> },
                    loadPlaybackSettings = {
                        PlaybackSettingsUiState(false, true, emptyList())
                    },
                    setEqualizerEnabled = { enabled ->
                        PlaybackSettingsUiState(enabled, true, emptyList())
                    },
                    replaceEqualizerCurve = {
                        PlaybackSettingsUiState(false, true, emptyList())
                    },
                    setGaplessEnabled = { enabled ->
                        PlaybackSettingsUiState(false, enabled, emptyList())
                    },
                )
            }
        }
        val miniPlayer = { compose.onNode(hasClickLabel("Open Now Playing")) }

        // Asked for, not answered: the frame stays empty rather than borrowing
        // a row from somewhere else.
        miniPlayer().assertDoesNotExist()

        compose.runOnIdle { pending.getValue(first.id)(first) }
        miniPlayer().assertTextContains("First Song")

        // The session moves on before the next row has been read.
        compose.runOnIdle {
            playback = playback.copy(currentTrackId = second.id, currentTrackUri = second.uri)
        }
        miniPlayer().assertTextContains("First Song")

        // A repeated old answer cannot dislodge the retained old row either.
        compose.runOnIdle { pending.getValue(first.id)(first) }
        miniPlayer().assertTextContains("First Song")

        compose.runOnIdle { pending.getValue(second.id)(second) }
        miniPlayer().assertTextContains("Second Song")

        // Playback ends. Whatever answers now belongs to nothing.
        compose.runOnIdle { playback = PlaybackUiState(ready = true) }
        compose.runOnIdle { pending.getValue(second.id)(second) }
        miniPlayer().assertDoesNotExist()
    }

    /**
     * The commit leg (a completed [onBackPressed]) is covered by
     * [miniPlayerOpensTheSheetBackClosesItAndTheLibraryKeepsWorking]. Gesture
     * navigation shows the progressed preview far more often than a completed
     * swipe, and that leg was untested: [PredictiveBackHandler] only calls
     * `close()` after its event flow completes, so a gesture held mid-swipe
     * must neither dismiss the sheet nor leave it sitting still.
     *
     * `OnBackPressedDispatcher.dispatchOnBackStarted`/`dispatchOnBackProgressed`
     * reach [PredictiveBackHandler]'s flow under Robolectric — confirmed by
     * driving them here — and the sheet's `graphicsLayer` translation is
     * visible to `getUnclippedBoundsInRoot()`, so the actual on-screen motion
     * is what gets asserted, not a proxy for it.
     */
    @Test
    fun predictiveBackProgressMovesTheSheetWithoutClosingIt() {
        var closed = false
        compose.setContent {
            RepriseTheme(nocturneForTests, darkPalette = true) {
                CompositionLocalProvider(LocalPlaybackControls provides RecordingPlaybackControls()) {
                    NowPlayingSheet(
                        track = testTrack(rating = 2),
                        playback = testPlayback(positionMs = 20_000),
                        close = { closed = true },
                    )
                }
            }
        }
        val transport = compose.onNodeWithTag("now-playing-transport")
        val restTop = transport.getUnclippedBoundsInRoot().top

        compose.runOnIdle {
            val dispatcher = compose.activity.onBackPressedDispatcher
            dispatcher.dispatchOnBackStarted(BackEventCompat(0f, 0f, 0f, BackEventCompat.EDGE_LEFT))
            dispatcher.dispatchOnBackProgressed(BackEventCompat(0f, 0f, 0.5f, BackEventCompat.EDGE_LEFT))
        }
        compose.waitForIdle()

        assertFalse(closed)
        transport.assertIsDisplayed()
        val progressedTop = transport.getUnclippedBoundsInRoot().top
        // A completed drag at halfway drives roughly 39dp of the 64dp full
        // step in practice; the threshold stays well clear of both that and
        // measurement noise so it fails if the translation stops tracking
        // backProgress, not if a rounding pixel wobbles.
        assertTrue(
            "progressed top $progressedTop should clear rest top $restTop by more than " +
                "${PROGRESS_MOTION_THRESHOLD_DP}dp",
            progressedTop.value > restTop.value + PROGRESS_MOTION_THRESHOLD_DP,
        )
    }

    /**
     * The abandoned-gesture leg of the same contract: a swipe released before
     * completion reaches [PredictiveBackHandler]'s `CancellationException`
     * catch, which must both leave the sheet open and reset `backProgress` to
     * 0f. Driven the same way as the progress case, then cancelled instead of
     * completed.
     */
    @Test
    fun predictiveBackCancelSnapsTheSheetFullyBackOpen() {
        var closed = false
        compose.setContent {
            RepriseTheme(nocturneForTests, darkPalette = true) {
                CompositionLocalProvider(LocalPlaybackControls provides RecordingPlaybackControls()) {
                    NowPlayingSheet(
                        track = testTrack(rating = 2),
                        playback = testPlayback(positionMs = 20_000),
                        close = { closed = true },
                    )
                }
            }
        }
        val transport = compose.onNodeWithTag("now-playing-transport")
        val restTop = transport.getUnclippedBoundsInRoot().top

        compose.runOnIdle {
            val dispatcher = compose.activity.onBackPressedDispatcher
            dispatcher.dispatchOnBackStarted(BackEventCompat(0f, 0f, 0f, BackEventCompat.EDGE_LEFT))
            dispatcher.dispatchOnBackProgressed(BackEventCompat(0f, 0f, 0.5f, BackEventCompat.EDGE_LEFT))
        }
        compose.waitForIdle()
        assertTrue(transport.getUnclippedBoundsInRoot().top.value > restTop.value + PROGRESS_MOTION_THRESHOLD_DP)

        compose.runOnIdle { compose.activity.onBackPressedDispatcher.dispatchOnBackCancelled() }
        compose.waitForIdle()

        assertFalse(closed)
        transport.assertIsDisplayed()
        assertEquals(restTop.value, transport.getUnclippedBoundsInRoot().top.value, 0.5f)
    }

    @Test
    fun equalizerEditWarnsBeforeReplacingTheCurveAndViewingWritesNothing() {
        val replacements = mutableListOf<List<EqualizerCurvePoint>>()
        compose.setContent {
            RepriseTheme(nocturneForTests, darkPalette = true) {
                PlaybackSettingsScreen(
                    state = PlaybackSettingsUiState(
                        equalizerEnabled = true,
                        gaplessEnabled = true,
                        equalizerBands = listOf(
                            EqualizerBandUi(125.0, -2.0, -12.0, 12.0),
                            EqualizerBandUi(1_000.0, 1.0, -12.0, 12.0),
                        ),
                    ),
                    themeSelection = nocturneForTests,
                    close = {},
                    setEqualizerEnabled = {},
                    replaceEqualizerCurve = { replacements += it },
                    setGaplessEnabled = {},
                    selectTheme = {},
                )
            }
        }

        assertTrue(replacements.isEmpty())
        compose.onNodeWithText("Crossfade").assertDoesNotExist()
        compose.onNodeWithText("ReplayGain").assertDoesNotExist()
        compose.onNodeWithText("Edit equalizer").performClick()
        compose.onNodeWithText(
            "Editing here replaces the saved equalizer curve with this device's bands.",
        ).assertIsDisplayed()
        assertTrue(replacements.isEmpty())

        compose.onNodeWithText("Continue").performClick()
        assertTrue(replacements.isEmpty())
        compose.onNodeWithContentDescription("125 Hz equalizer band")
            .performSemanticsAction(SemanticsActions.SetProgress) { setProgress ->
                setProgress(3f)
            }

        assertEquals(1, replacements.size)
        assertEquals(listOf(125.0, 1_000.0), replacements.single().map { it.frequencyHz })
        assertEquals(3.0, replacements.single().first().gainDb, 0.01)
    }

    @Test
    fun overflowRoutesToSettingsAndKeepsThemeChoicesOutOfTheMenu() {
        val browse = LibraryScreenState.Browse(
            titles = LibraryWindow.empty(),
            artists = LibraryWindow.empty(),
        )
        compose.setContent {
            RepriseTheme(nocturneForTests, darkPalette = true) {
                BrowseScreen(
                    state = browse,
                    playback = PlaybackUiState(),
                    playbackSettingsRevision = 0,
                    chooseFolder = {},
                    rescan = {},
                    themeSelection = nocturneForTests,
                    selectTheme = {},
                    searchTitles = { _, _ -> browse.titles },
                    searchAlbums = { _, _ -> LibraryWindow.empty() },
                    listArtists = { browse.artists },
                    openAlbum = { error("Album navigation is outside this test") },
                    listAlbumTracks = { _, _ -> LibraryWindow.empty() },
                    loadTrack = { _, deliver -> deliver(null) },
                    playTracks = { _, _ -> },
                    loadPlaybackSettings = {
                        PlaybackSettingsUiState(false, true, emptyList())
                    },
                    setEqualizerEnabled = { enabled ->
                        PlaybackSettingsUiState(enabled, true, emptyList())
                    },
                    replaceEqualizerCurve = {
                        PlaybackSettingsUiState(false, true, emptyList())
                    },
                    setGaplessEnabled = { enabled ->
                        PlaybackSettingsUiState(false, enabled, emptyList())
                    },
                )
            }
        }

        compose.onNodeWithContentDescription("Library actions").performClick()
        compose.onNodeWithText("Nocturne").assertDoesNotExist()
        compose.onNodeWithText("Settings").performClick()
        compose.onNodeWithText("Appearance").assertIsDisplayed()
        compose.onNodeWithText("Nocturne").assertIsDisplayed()
    }

    /**
     * A rotation recreates the activity: `settingsVisible` is saveable and comes
     * back true, the loaded settings are a plain `remember` and come back null.
     * The screen used to render nothing at all in that state — a full-screen
     * surface with no header and no way back — and if the reload then failed,
     * the old error path folded `null?.copy(...)` back to null and it stayed
     * that way. Restoring `settingsState?.let { ... }`, or the `?:`-less error
     * path, turns this red.
     */
    @Test
    fun settingsSurviveARotationEvenWhenTheReloadFails() {
        val browse = LibraryScreenState.Browse(
            titles = LibraryWindow.empty(),
            artists = LibraryWindow.empty(),
        )
        var opened = false
        val restoration = StateRestorationTester(compose)
        restoration.setContent {
            RepriseTheme(nocturneForTests, darkPalette = true) {
                BrowseScreen(
                    state = browse,
                    playback = PlaybackUiState(),
                    playbackSettingsRevision = 0,
                    chooseFolder = {},
                    rescan = {},
                    themeSelection = nocturneForTests,
                    selectTheme = {},
                    searchTitles = { _, _ -> browse.titles },
                    searchAlbums = { _, _ -> LibraryWindow.empty() },
                    listArtists = { browse.artists },
                    openAlbum = { error("Album navigation is outside this test") },
                    listAlbumTracks = { _, _ -> LibraryWindow.empty() },
                    loadTrack = { _, deliver -> deliver(null) },
                    playTracks = { _, _ -> },
                    loadPlaybackSettings = {
                        // Open once, then behave like a service that has not
                        // rebound yet — the state this screen used to keep.
                        if (opened) error("playback is still connecting")
                        opened = true
                        PlaybackSettingsUiState(false, true, emptyList())
                    },
                    setEqualizerEnabled = { PlaybackSettingsUiState(it, true, emptyList()) },
                    replaceEqualizerCurve = { PlaybackSettingsUiState(false, true, emptyList()) },
                    setGaplessEnabled = { PlaybackSettingsUiState(false, it, emptyList()) },
                )
            }
        }
        compose.onNodeWithContentDescription("Library actions").performClick()
        compose.onNodeWithText("Settings").performClick()
        compose.onNodeWithContentDescription("Back to Library").assertIsDisplayed()

        restoration.emulateSavedInstanceStateRestore()

        compose.onNodeWithContentDescription("Back to Library").assertIsDisplayed()
        compose.onNodeWithText("Settings").assertIsDisplayed()
        compose.onNodeWithText(
            "Could not refresh playback settings: playback is still connecting",
        ).assertIsDisplayed()
    }

    /** The waiting screen the rotation lands on before the reload answers. */
    @Test
    fun theSettingsScreenWithoutItsStateStillOffersAWayBack() {
        var closed = false
        compose.setContent {
            RepriseTheme(nocturneForTests, darkPalette = true) {
                PlaybackSettingsLoading(close = { closed = true })
            }
        }

        compose.onNodeWithText("Reading playback settings…").assertIsDisplayed()
        compose.onNodeWithContentDescription("Back to Library").performClick()
        assertTrue(closed)
    }

    /**
     * `engine` stays null forever when the device refuses the effect and nothing
     * retries, so "start playback" would be a cause the screen knows to be false
     * while a track is playing.
     */
    @Test
    fun anAbsentDeviceEqualizerIsNotReportedAsAbsentPlayback() {
        compose.setContent {
            RepriseTheme(nocturneForTests, darkPalette = true) {
                PlaybackSettingsScreen(
                    state = PlaybackSettingsUiState(
                        equalizerEnabled = true,
                        gaplessEnabled = true,
                        equalizerBands = emptyList(),
                        equalizerBandsAbsence =
                        EqualizerBandsAbsence.NO_EQUALIZER_ON_THIS_DEVICE,
                    ),
                    themeSelection = nocturneForTests,
                    close = {},
                    setEqualizerEnabled = {},
                    replaceEqualizerCurve = {},
                    setGaplessEnabled = {},
                    selectTheme = {},
                )
            }
        }

        compose.onNodeWithText("This device provided no equalizer for what is playing.")
            .assertIsDisplayed()
        compose.onNodeWithText("Start playback to read this device's equalizer bands.")
            .assertDoesNotExist()
    }

    /**
     * A scan hands the screen a whole new catalog: four freshly read windows,
     * none of them refined, and a `loadedTabs` that calls all four loaded. The
     * refinement is applied again from there — but only to the tab the listener
     * is standing in, because that is the one the search fills.
     *
     * Asking whether the *query* changed cannot catch this: it did not change,
     * the library underneath it did. So every other tab has to lose its claim
     * to be loaded, or the one they wander back to answers with the whole
     * library while their query is still sitting in the field. That is the
     * original complaint, arrived at by a second road.
     */
    @Test
    fun aScanLeavesNoTabAnsweringWithTheWholeLibraryWhileTheQueryStands() {
        val slowSong = testTrack(rating = 3).copy(id = 1, title = "Slow Song")
        val loudSong = testTrack(rating = 3).copy(
            id = 2,
            uri = "content://provider/document/loud.flac",
            title = "Loud Song",
        )
        // What the scan finds the second time round. Its only job is to change
        // the catalog's shape, so the screen cannot take up the windows it had.
        val foundSong = testTrack(rating = 3).copy(
            id = 3,
            uri = "content://provider/document/found.flac",
            title = "Loud Song Two",
        )
        val slowAlbum = testAlbumNamed("Slow Album")
        val loudAlbum = testAlbumNamed("Loud Album")
        var songs by mutableStateOf(listOf(slowSong, loudSong))

        fun matching(text: String) = songs
            .filter { text.isBlank() || it.title.contains(text, ignoreCase = true) }
            .let { LibraryWindow(total = it.size.toLong(), rows = it, hasMore = false) }

        fun matchingAlbums(text: String) = listOf(slowAlbum, loudAlbum)
            .filter { text.isBlank() || it.title.contains(text, ignoreCase = true) }
            .let { LibraryWindow(total = it.size.toLong(), rows = it, hasMore = false) }

        var browse by mutableStateOf(
            LibraryScreenState.Browse(
                titles = matching(""),
                artists = LibraryWindow.empty(),
            ),
        )
        compose.setContent {
            RepriseTheme(nocturneForTests, darkPalette = true) {
                BrowseScreen(
                    state = browse,
                    playback = testPlayback(positionMs = 0).copy(
                        currentIndex = null,
                        currentTrackId = null,
                        currentTrackUri = null,
                    ),
                    playbackSettingsRevision = 0,
                    chooseFolder = {},
                    rescan = {},
                    themeSelection = nocturneForTests,
                    selectTheme = {},
                    searchTitles = { text, _ -> matching(text) },
                    searchAlbums = { text, _ -> matchingAlbums(text) },
                    listArtists = { LibraryWindow.empty() },
                    openAlbum = { error("Album navigation is outside this test") },
                    listAlbumTracks = { _, _ -> LibraryWindow.empty() },
                    loadTrack = { _, deliver -> deliver(null) },
                    playTracks = { _, _ -> },
                    loadPlaybackSettings = {
                        PlaybackSettingsUiState(false, true, emptyList())
                    },
                    setEqualizerEnabled = { PlaybackSettingsUiState(false, true, emptyList()) },
                    replaceEqualizerCurve = { PlaybackSettingsUiState(false, true, emptyList()) },
                    setGaplessEnabled = { PlaybackSettingsUiState(false, true, emptyList()) },
                )
            }
        }

        compose.onNodeWithContentDescription("Search library").performClick()
        compose.onNodeWithText("Search titles").performTextInput("slow")
        compose.waitForIdle()
        compose.onNodeWithText("Loud Song").assertDoesNotExist()

        compose.onNodeWithText("Artists").performClick()
        compose.waitForIdle()
        compose.onNodeWithText("Slow Album").assertIsDisplayed()
        compose.onNodeWithText("Loud Album").assertDoesNotExist()

        // The scan lands while Artists search is on screen.
        songs = listOf(slowSong, loudSong, foundSong)
        browse = LibraryScreenState.Browse(
            titles = matching(""),
            artists = LibraryWindow.empty(),
        )
        compose.waitForIdle()

        compose.onNodeWithText("Titles").performClick()
        compose.waitForIdle()

        compose.onNodeWithText("Search titles").assertTextContains("slow")
        compose.onNodeWithText("Slow Song").assertIsDisplayed()
        compose.onNodeWithText("Loud Song").assertDoesNotExist()
        compose.onNodeWithText("Loud Song Two").assertDoesNotExist()
    }

    private fun SemanticsNodeInteraction.progress(): Float =
        fetchSemanticsNode().config[SemanticsProperties.ProgressBarRangeInfo].current

    private fun heart(description: String): SemanticsNodeInteraction =
        compose.onNodeWithContentDescription(description)
}

/** Matches the node offering a click labelled [label], however it is described. */
private fun hasClickLabel(label: String) =
    SemanticsMatcher("${SemanticsActions.OnClick.name} is labelled \"$label\"") { node ->
        node.config.getOrNull(SemanticsActions.OnClick)?.label == label
    }

/** See [ComposeBehaviorTest.predictiveBackProgressMovesTheSheetWithoutClosingIt]. */
private const val PROGRESS_MOTION_THRESHOLD_DP = 24f

private fun testPlayback(positionMs: Long) = PlaybackUiState(
    ready = true,
    state = AndroidPlaybackState.PAUSED,
    currentIndex = 0,
    currentTrackId = 830,
    currentTrackUri = "content://provider/document/song.flac",
    positionMs = positionMs,
    durationMs = 100_000,
)

private fun testAlbumNamed(title: String) = LibraryAlbum(
    title = title,
    artist = "Band",
    representativeUri = "content://provider/document/${title.lowercase().replace(' ', '-')}.flac",
    trackCount = 4,
    year = 2001,
    totalDurationMs = 900_000,
)

private fun testTrack(rating: Int) = LibraryTrack(
    id = 830,
    uri = "content://provider/document/song.flac",
    title = "Song",
    artist = "Artist",
    album = "Album",
    durationMs = 100_000,
    playCount = 27,
    rating = rating,
)

private class RecordingPlaybackControls(
    private val ratingFailure: String? = null,
    /**
     * False stands in for the writer thread: the tap is recorded, and the
     * answer arrives only when [answerPending] is called, the way it does once
     * the database has actually agreed.
     */
    private val answerImmediately: Boolean = true,
) : PlaybackControls by DisconnectedPlaybackControls {
    val seekPositions = mutableListOf<Long>()
    val ratingRequests = mutableListOf<Pair<Long, Int>>()
    private val unanswered = mutableListOf<(String?) -> Unit>()

    override fun seekTo(positionMs: Long) {
        seekPositions += positionMs
    }

    override fun setFavourite(trackId: Long, favourite: Boolean, report: (String?) -> Unit) {
        val rating = if (favourite) 5 else 0
        ratingRequests += trackId to rating
        if (answerImmediately) {
            report(ratingFailure)
        } else {
            unanswered += report
        }
    }

    fun answerPending(message: String?) {
        val reports = unanswered.toList()
        unanswered.clear()
        reports.forEach { report -> report(message) }
    }
}
