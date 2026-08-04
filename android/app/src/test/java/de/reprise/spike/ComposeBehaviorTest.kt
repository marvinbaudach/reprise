package de.reprise.spike

import androidx.activity.BackEventCompat
import androidx.activity.ComponentActivity
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
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performSemanticsAction
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
    fun failedRatingShowsTheErrorWithoutMovingTheStars() {
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

        // The stars carry the rating as their state, so reading it back off any
        // of them is reading what the control shows.
        star(2).assertRating(2)
        star(4).assertRating(2)

        star(4).performClick()

        compose.onNodeWithText(failure).assertIsDisplayed()
        star(2).assertRating(2)
        star(4).assertRating(2)
        assertEquals(listOf(830L to 4), controls.ratingRequests)
    }

    @Test
    fun miniPlayerOpensTheSheetBackClosesItAndTheLibraryKeepsWorking() {
        val tracks = listOf(
            testTrack(rating = 2).copy(id = 830, title = "First Song"),
            testTrack(rating = 4).copy(
                id = 831,
                uri = "content://provider/document/second.flac",
                title = "Second Song",
            ),
        )
        val browse = LibraryScreenState.Browse(
            titles = LibraryWindow(total = 2, rows = tracks, hasMore = false),
            albums = LibraryWindow.empty(),
            artists = LibraryWindow.empty(),
        )
        val playedIndices = mutableListOf<Int>()
        var playback by mutableStateOf(testPlayback(positionMs = 20_000))
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
                    listAlbums = { browse.albums },
                    listArtists = { browse.artists },
                    openAlbum = { error("Album navigation is outside this test") },
                    listAlbumTracks = { _, _ -> LibraryWindow.empty() },
                    playTracks = { selection, _ ->
                        playedIndices += selection.startIndex
                        playback = playback.copy(currentIndex = selection.startIndex)
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
        // is merged into what it announces. The rating and the play count earn
        // their place there; the cover does not — "Album artwork" would sit
        // where the song belongs. Pinned exactly, so a cover that starts
        // describing itself again fails here.
        compose.onNodeWithText("First Song")
            .assertContentDescriptionEquals("2 of 5 stars", "27 plays")

        compose.onNodeWithText("First Song").performClick()
        // Found by the action it offers rather than by a description of its
        // own: a content description on this node would be merged over the
        // track it announces, and announcing the track is the point of it.
        val miniPlayer = compose.onNode(hasClickLabel("Open Now Playing"))
        miniPlayer.assertTextContains("First Song")
        miniPlayer.assertTextContains("Artist")
        miniPlayer.assert(SemanticsMatcher.keyNotDefined(SemanticsProperties.ContentDescription))

        miniPlayer.performClick()
        compose.onNodeWithContentDescription("Collapse Now Playing").assertIsDisplayed()
        compose.onAllNodesWithText("First Song").assertCountEquals(3)

        // The full-screen sheet should intercept pointer hit testing. Calling
        // the retained row's public action proves its state and wiring stayed
        // alive underneath rather than replacing the Library composition.
        compose.onNodeWithText("Second Song")
            .performSemanticsAction(SemanticsActions.OnClick) { click -> click() }
        compose.onAllNodesWithText("Second Song").assertCountEquals(3)
        assertEquals(listOf(0, 1), playedIndices)

        compose.runOnIdle { compose.activity.onBackPressedDispatcher.onBackPressed() }
        compose.onNodeWithContentDescription("Collapse Now Playing").assertDoesNotExist()
        compose.onAllNodesWithText("Second Song").assertCountEquals(2)
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
        val collapse = compose.onNodeWithContentDescription("Collapse Now Playing")
        val restTop = collapse.getUnclippedBoundsInRoot().top

        compose.runOnIdle {
            val dispatcher = compose.activity.onBackPressedDispatcher
            dispatcher.dispatchOnBackStarted(BackEventCompat(0f, 0f, 0f, BackEventCompat.EDGE_LEFT))
            dispatcher.dispatchOnBackProgressed(BackEventCompat(0f, 0f, 0.5f, BackEventCompat.EDGE_LEFT))
        }
        compose.waitForIdle()

        assertFalse(closed)
        collapse.assertIsDisplayed()
        val progressedTop = collapse.getUnclippedBoundsInRoot().top
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
        val collapse = compose.onNodeWithContentDescription("Collapse Now Playing")
        val restTop = collapse.getUnclippedBoundsInRoot().top

        compose.runOnIdle {
            val dispatcher = compose.activity.onBackPressedDispatcher
            dispatcher.dispatchOnBackStarted(BackEventCompat(0f, 0f, 0f, BackEventCompat.EDGE_LEFT))
            dispatcher.dispatchOnBackProgressed(BackEventCompat(0f, 0f, 0.5f, BackEventCompat.EDGE_LEFT))
        }
        compose.waitForIdle()
        assertTrue(collapse.getUnclippedBoundsInRoot().top.value > restTop.value + PROGRESS_MOTION_THRESHOLD_DP)

        compose.runOnIdle { compose.activity.onBackPressedDispatcher.dispatchOnBackCancelled() }
        compose.waitForIdle()

        assertFalse(closed)
        collapse.assertIsDisplayed()
        assertEquals(restTop.value, collapse.getUnclippedBoundsInRoot().top.value, 0.5f)
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
            albums = LibraryWindow.empty(),
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
                    listAlbums = { browse.albums },
                    listArtists = { browse.artists },
                    openAlbum = { error("Album navigation is outside this test") },
                    listAlbumTracks = { _, _ -> LibraryWindow.empty() },
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

    private fun SemanticsNodeInteraction.progress(): Float =
        fetchSemanticsNode().config[SemanticsProperties.ProgressBarRangeInfo].current

    private fun star(star: Int): SemanticsNodeInteraction =
        compose.onNodeWithContentDescription("Rate $star of 5 stars")

    private fun SemanticsNodeInteraction.assertRating(rating: Int): SemanticsNodeInteraction =
        assert(
            SemanticsMatcher.expectValue(
                SemanticsProperties.StateDescription,
                "Rated $rating of 5",
            ),
        )
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
    positionMs = positionMs,
    durationMs = 100_000,
    playPauseLabel = "Play",
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
) : PlaybackControls by DisconnectedPlaybackControls {
    val seekPositions = mutableListOf<Long>()
    val ratingRequests = mutableListOf<Pair<Long, Int>>()

    override fun seekTo(positionMs: Long) {
        seekPositions += positionMs
    }

    override fun setRating(trackId: Long, rating: Int): String? {
        ratingRequests += trackId to rating
        return ratingFailure
    }
}
