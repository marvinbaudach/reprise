package de.reprise.spike

import androidx.activity.ComponentActivity
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.semantics.ProgressBarRangeInfo
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.SemanticsNodeInteraction
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import de.reprise.spike.settings.OnlineSourcesSettingsPage
import de.reprise.spike.ui.theme.RepriseTheme
import java.util.concurrent.ConcurrentLinkedQueue
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.reprise_android_ffi.AndroidColorScheme

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36], qualifiers = "w500dp-h1000dp")
class ArtistPhotoProgressBarTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    fun preparingHasNoCounterAndUsesIndeterminateProgress() {
        show(ArtistPhotoProgress(1, ArtistPhotoProgressPhase.PREPARING, 0, 0, 0))

        compose.onNodeWithText("Preparing artist photos").assertIsDisplayed()
        compose.onNodeWithTag("artist-photo-progress-counter").assertDoesNotExist()
        compose.onNodeWithTag("artist-photo-progress-track").assertIsDisplayed()
    }

    @Test
    fun runningShowsTheDownloadedCountAndDeterminateProgress() {
        show(ArtistPhotoProgress(2, ArtistPhotoProgressPhase.RUNNING, 128, 0, 412))

        compose.onNodeWithText("Downloading artist photos").assertIsDisplayed()
        compose.onNodeWithText("128 / 412").assertIsDisplayed()
        compose.onNodeWithTag("artist-photo-progress-track")
            .assertProgress(128f / 412f)
        compose.onNodeWithContentDescription("Artist photos, 128 of 412 downloaded")
            .assertIsDisplayed()
    }

    @Test
    fun successfulCompletionIsAllTealAndHasNoFailureLine() {
        show(ArtistPhotoProgress(3, ArtistPhotoProgressPhase.COMPLETE, 412, 0, 412))

        compose.onNodeWithText("Artist photos complete").assertIsDisplayed()
        compose.onNodeWithText("412 / 412").assertIsDisplayed()
        compose.onNodeWithTag("artist-photo-progress-track").assertProgress(1f)
        compose.onNodeWithTag("artist-photo-progress-failure").assertDoesNotExist()
    }

    @Test
    fun failedCompletionUsesTheDesignCounts() {
        show(ArtistPhotoProgress(4, ArtistPhotoProgressPhase.COMPLETE, 397, 15, 412))

        compose.onNodeWithText("397 / 412").assertIsDisplayed()
        compose.onNodeWithText("15 without a photo").assertIsDisplayed()
        compose.onNodeWithTag("artist-photo-progress-track").assertProgress(1f)
    }

    @Test
    fun pausedKeepsItsLastDeterminateStand() {
        show(ArtistPhotoProgress(5, ArtistPhotoProgressPhase.PAUSED, 128, 15, 412))

        compose.onNodeWithText("Waiting for a connection").assertIsDisplayed()
        compose.onNodeWithText("128 / 412").assertIsDisplayed()
        compose.onNodeWithTag("artist-photo-progress-track")
            .assertProgress(143f / 412f)
    }

    @Test
    fun noRunCreatesNoNode() {
        show(null)

        compose.onNodeWithTag("artist-photo-progress").assertDoesNotExist()
    }

    @Test
    fun dismissalSticksForOneRunAndClearsForTheNext() {
        val viewModel = MobileSurfaceViewModel()
        viewModel.acceptArtistPhotoProgress(running(runId = 9))

        viewModel.dismissArtistPhotoProgress()
        viewModel.acceptArtistPhotoProgress(running(runId = 9, done = 2))
        assertEquals(null, viewModel.visibleArtistPhotoProgress)

        viewModel.acceptArtistPhotoProgress(running(runId = 10))
        assertEquals(10L, viewModel.visibleArtistPhotoProgress?.runId)
    }

    @Test
    fun aBackgroundSnapshotIsPostedBeforeItMutatesComposeState() {
        val posted = ConcurrentLinkedQueue<() -> Unit>()
        var snapshot = running(runId = 20)
        val viewModel = MobileSurfaceViewModel()
        viewModel.bindArtistPhotoBackfill(
            snapshot = { snapshot },
            start = {},
            cancel = {},
            postToMain = posted::add,
        )
        posted.remove().invoke()
        assertEquals(20L, viewModel.visibleArtistPhotoProgress?.runId)
        snapshot = running(runId = 21)

        Thread(viewModel::startArtistPhotoBackfill).also {
            it.start()
            it.join()
        }

        assertEquals(20L, viewModel.visibleArtistPhotoProgress?.runId)
        assertEquals(1, posted.size)
        posted.remove().invoke()
        assertEquals(21L, viewModel.visibleArtistPhotoProgress?.runId)
    }

    @Test
    fun dismissButtonReportsHideProgress() {
        var dismissals = 0
        show(running(runId = 7), dismiss = { dismissals += 1 })

        compose.onNodeWithContentDescription("Hide progress").performClick()

        assertEquals(1, dismissals)
    }

    @Test
    fun successfulCompletionDismissesAfterFourSeconds() {
        var dismissals = 0
        compose.mainClock.autoAdvance = false
        show(
            ArtistPhotoProgress(12, ArtistPhotoProgressPhase.COMPLETE, 412, 0, 412),
            dismiss = { dismissals += 1 },
        )

        compose.mainClock.advanceTimeBy(4_001)
        compose.waitForIdle()

        assertEquals(1, dismissals)
    }

    @Test
    fun onlineSourcesUsesTheSameProgressLabels() {
        compose.setContent {
            RepriseTheme(theme, darkPalette = true) {
                OnlineSourcesSettingsPage(
                    enabled = true,
                    setEnabled = {},
                    progress = running(runId = 8),
                    dismissProgress = {},
                    back = {},
                )
            }
        }

        compose.onNodeWithText("Downloading artist photos").assertIsDisplayed()
        compose.onNodeWithText("1 / 412").assertIsDisplayed()
    }

    @Test
    fun browseUsesTheSameProgressLabelsAboveItsPager() {
        val viewModel = MobileSurfaceViewModel().apply {
            acceptArtistPhotoProgress(running(runId = 11))
        }
        val browse = LibraryScreenState.Browse(
            titles = LibraryWindow.empty(),
            artists = LibraryWindow.empty(),
        )
        compose.setContent {
            RepriseTheme(theme, darkPalette = true) {
                BrowseScreen(
                    state = browse,
                    playback = PlaybackUiState().libraryPlayback(),
                    playbackSettingsRevision = 0,
                    surfaceState = viewModel,
                    chooseFolder = {},
                    rescan = {},
                    searchTitles = { _, _ -> LibraryWindow.empty() },
                    searchAlbums = { _, _ -> LibraryWindow.empty() },
                    listArtists = { LibraryWindow.empty() },
                    openAlbum = { error("Album navigation is outside this test") },
                    listAlbumTracks = { _, _ -> LibraryWindow.empty() },
                    loadTrack = { _, deliver -> deliver(null) },
                    playTracks = { _, _ -> },
                    loadPlaybackSettings = { PlaybackSettingsUiState(false, true, emptyList()) },
                    setEqualizerEnabled = { PlaybackSettingsUiState(it, true, emptyList()) },
                    replaceEqualizerCurve = {
                        PlaybackSettingsUiState(false, true, emptyList())
                    },
                    setGaplessEnabled = { PlaybackSettingsUiState(false, it, emptyList()) },
                    themeSelection = theme,
                    selectTheme = {},
                )
            }
        }

        compose.onNodeWithText("Downloading artist photos").assertIsDisplayed()
        compose.onNodeWithText("1 / 412").assertIsDisplayed()
        compose.onNodeWithTag("library-destination-pager").assertIsDisplayed()
    }

    private fun show(
        initial: ArtistPhotoProgress?,
        dismiss: () -> Unit = {},
    ) {
        val state = mutableStateOf(initial)
        compose.setContent {
            RepriseTheme(theme, darkPalette = true) {
                ArtistPhotoProgressBar(progress = state.value, dismiss = dismiss)
            }
        }
    }

    private fun running(runId: Long, done: Long = 1) = ArtistPhotoProgress(
        runId = runId,
        phase = ArtistPhotoProgressPhase.RUNNING,
        done = done,
        failed = 0,
        total = 412,
    )

    private val theme = MobileThemeSelection(
        palette = MobileTheme.NOCTURNE,
        colorScheme = AndroidColorScheme.SYSTEM,
        dynamicAvailable = false,
    )
}

private fun SemanticsNodeInteraction.assertProgress(value: Float) = assert(
    SemanticsMatcher.expectValue(
        SemanticsProperties.ProgressBarRangeInfo,
        ProgressBarRangeInfo(value, 0f..1f, 0),
    ),
)
