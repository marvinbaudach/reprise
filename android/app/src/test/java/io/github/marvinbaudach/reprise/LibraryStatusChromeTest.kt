package io.github.marvinbaudach.reprise

import androidx.activity.ComponentActivity
import androidx.compose.runtime.MutableState
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.click
import io.github.marvinbaudach.reprise.ui.theme.RepriseTheme
import kotlin.math.roundToInt
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.reprise_android_ffi.AndroidColorScheme
import java.util.concurrent.atomic.AtomicInteger

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36], qualifiers = "w500dp-h1000dp")
class LibraryStatusChromeTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    fun artworkProgressDoesNotMoveTheList() {
        val viewModel = MobileSurfaceViewModel()
        showArtists(viewModel)
        val pagerTop = topInPixels("library-destination-pager")
        val firstRowTop = textTopInPixels(FIRST_ARTIST)

        compose.runOnIdle {
            viewModel.acceptArtistPhotoProgress(progress(ArtistPhotoProgressPhase.PREPARING))
        }
        compose.onNodeWithText("6 artists · Preparing artwork").assertIsDisplayed()
        assertListTops(pagerTop, firstRowTop)

        compose.runOnIdle {
            viewModel.acceptArtistPhotoProgress(progress(ArtistPhotoProgressPhase.RUNNING, done = 2))
        }
        compose.onNodeWithText("6 artists · Artwork 2/6").assertIsDisplayed()
        compose.onNodeWithTag("artist-photo-progress-track").assertIsDisplayed()
        assertListTops(pagerTop, firstRowTop)

        compose.runOnIdle {
            viewModel.acceptArtistPhotoProgress(progress(ArtistPhotoProgressPhase.COMPLETE, done = 6))
        }
        compose.onNodeWithText("6 artists").assertIsDisplayed()
        assertListTops(pagerTop, firstRowTop)

        compose.mainClock.advanceTimeBy(4_001)
        compose.waitForIdle()
        compose.mainClock.advanceTimeBy(DELETION_LINE_FADE_MS.toLong() + 1)
        compose.waitForIdle()
        compose.onNodeWithTag("artist-photo-progress-track").assertDoesNotExist()
        assertListTops(pagerTop, firstRowTop)
    }

    @Test
    fun anErrorDoesNotMoveTheListAndItsCloseButtonHidesIt() {
        val viewModel = MobileSurfaceViewModel()
        val harness = showArtists(viewModel)
        val pagerTop = topInPixels("library-destination-pager")
        val firstRowTop = textTopInPixels(FIRST_ARTIST)

        compose.runOnIdle {
            harness.playback.value = LibraryPlayback(error = "Playback failed")
            harness.browse.value = harness.browse.value.copy(message = "Browse failed")
        }
        compose.onNode(hasTestTag("library-status-error") and hasText("Browse failed"))
            .assertIsDisplayed()
        assertListTops(pagerTop, firstRowTop)

        compose.onNodeWithContentDescription("Dismiss").performClick()
        compose.onNode(hasTestTag("library-status-error") and hasText("Playback failed"))
            .assertIsDisplayed()
        assertListTops(pagerTop, firstRowTop)

        compose.onNodeWithContentDescription("Dismiss").performClick()
        compose.onNodeWithTag("library-status-error").assertDoesNotExist()
        assertListTops(pagerTop, firstRowTop)
    }

    @Test
    fun aTapBesideTheCloseButtonReachesTheRowUnderneath() {
        val viewModel = MobileSurfaceViewModel()
        val openedArtists = AtomicInteger()
        val harness = showArtists(viewModel, openedArtists)
        compose.runOnIdle {
            harness.browse.value = harness.browse.value.copy(message = "Browse failed")
        }
        val pill = compose.onNodeWithTag("library-status-error").getUnclippedBoundsInRoot()
        val x = pill.left.value.toPixels() + 24f.toPixels()
        val y = ((pill.top.value + pill.bottom.value) / 2f).toPixels()

        compose.onRoot().performTouchInput { click(Offset(x.toFloat(), y.toFloat())) }

        compose.waitUntil(5_000) { openedArtists.get() == 1 }
        assertEquals(1, openedArtists.get())
    }

    @Test
    fun onADetailPageTheStatusSitsBelowTheHeader() {
        val viewModel = MobileSurfaceViewModel()
        showArtists(viewModel)
        compose.onNodeWithText(FIRST_ARTIST).performClick()
        compose.onNodeWithTag("artist-detail-header").assertIsDisplayed()
        compose.runOnIdle { viewModel.begin("Deleting 13 tracks…") }
        val headerBottom = bottomInPixels("artist-detail-header")

        val detailPillTop = topInPixels("deletion-message-line")
        org.junit.Assert.assertTrue(detailPillTop >= headerBottom)

        compose.runOnIdle { viewModel.selectTab(BrowseTab.TITLES) }
        compose.waitForIdle()
        val pagerTop = topInPixels("library-destination-pager")
        assertEquals(pagerTop + 8f.toPixels(), topInPixels("deletion-message-line"))
    }

    @Test
    fun artworkStopActionExistsOnlyWhileRunningAndHidesItsChrome() {
        val viewModel = MobileSurfaceViewModel()
        val stops = AtomicInteger()
        showArtists(viewModel, stopArtworkDownload = { stops.incrementAndGet() })
        compose.onNodeWithContentDescription("Library actions").performClick()
        compose.onNodeWithText("Stop artwork download").assertDoesNotExist()
        compose.runOnIdle { compose.activity.onBackPressedDispatcher.onBackPressed() }
        compose.runOnIdle {
            viewModel.acceptArtistPhotoProgress(progress(ArtistPhotoProgressPhase.RUNNING, done = 2))
        }

        compose.onNodeWithContentDescription("Library actions").performClick()
        compose.onNodeWithText("Stop artwork download").assertIsDisplayed().performClick()

        assertEquals(1, stops.get())
        compose.onNodeWithTag("artist-photo-progress-track").assertDoesNotExist()
        compose.onNodeWithText("6 artists").assertIsDisplayed()
    }

    private fun showArtists(
        viewModel: MobileSurfaceViewModel,
        openedArtists: AtomicInteger = AtomicInteger(),
        stopArtworkDownload: (() -> Unit)? = null,
    ): Harness {
        viewModel.selectTab(BrowseTab.ARTISTS)
        val artists = (1..6).map { index ->
            LibraryArtist(
                name = "Artist $index",
                trackCount = 3,
                albumCount = 1,
                representativeUri = "content://artists/$index",
            )
        }
        val artistWindow = LibraryWindow(
            total = artists.size.toLong(),
            rows = artists,
            hasMore = false,
        )
        val browseState = mutableStateOf(
            LibraryScreenState.Browse(
                titles = LibraryWindow.empty(),
                artists = artistWindow,
            ),
        )
        val playbackState = mutableStateOf(LibraryPlayback())
        compose.setContent {
            RepriseTheme(theme, darkPalette = true) {
                BrowseScreen(
                    state = browseState.value,
                    playback = playbackState.value,
                    playbackSettingsRevision = 0,
                    surfaceState = viewModel,
                    chooseFolder = {},
                    rescan = {},
                    searchTitles = { _, _ -> LibraryWindow.empty() },
                    listArtists = { artistWindow },
                    openArtist = {
                        openedArtists.incrementAndGet()
                        ArtistTrackList(artist = it)
                    },
                    openAlbum = { error("Album navigation is outside this test") },
                    listAlbumTracks = { _, _ -> LibraryWindow.empty() },
                    loadTrack = { _, deliver -> deliver(null) },
                    playTracks = { _, _ -> },
                    loadPlaybackSettings = {
                        PlaybackSettingsUiState(false, true, emptyList())
                    },
                    setEqualizerEnabled = { PlaybackSettingsUiState(it, true, emptyList()) },
                    replaceEqualizerCurve = {
                        PlaybackSettingsUiState(false, true, emptyList())
                    },
                    setGaplessEnabled = { PlaybackSettingsUiState(false, it, emptyList()) },
                    themeSelection = theme,
                    selectTheme = {},
                    stopArtistPhotoDownload = stopArtworkDownload,
                )
            }
        }
        compose.onNodeWithText(FIRST_ARTIST).assertIsDisplayed()
        return Harness(browseState, playbackState)
    }

    private fun assertListTops(pagerTop: Int, firstRowTop: Int) {
        assertEquals(pagerTop, topInPixels("library-destination-pager"))
        assertEquals(firstRowTop, textTopInPixels(FIRST_ARTIST))
    }

    private fun topInPixels(tag: String): Int = compose.onNodeWithTag(tag)
        .getUnclippedBoundsInRoot().top.value.toPixels()

    private fun textTopInPixels(text: String): Int = compose.onNodeWithText(text)
        .getUnclippedBoundsInRoot().top.value.toPixels()

    private fun bottomInPixels(tag: String): Int = compose.onNodeWithTag(tag)
        .getUnclippedBoundsInRoot().bottom.value.toPixels()

    private fun Float.toPixels(): Int =
        (this * compose.activity.resources.displayMetrics.density).roundToInt()

    private fun progress(
        phase: ArtistPhotoProgressPhase,
        done: Long = 0,
    ) = ArtistPhotoProgress(17, phase, done, 0, 6)

    private val theme = MobileThemeSelection(
        palette = MobileTheme.NOCTURNE,
        colorScheme = AndroidColorScheme.SYSTEM,
        dynamicAvailable = false,
    )

    private companion object {
        const val FIRST_ARTIST = "Artist 1"
    }

    private data class Harness(
        val browse: MutableState<LibraryScreenState.Browse>,
        val playback: MutableState<LibraryPlayback>,
    )
}
