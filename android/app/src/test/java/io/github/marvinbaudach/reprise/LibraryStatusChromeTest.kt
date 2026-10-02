package io.github.marvinbaudach.reprise

import androidx.activity.ComponentActivity
import androidx.compose.runtime.CompositionLocalProvider
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
import org.junit.Assert.assertTrue
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
        compose.onNodeWithTag("artist-photo-progress-track").assertExists()
        assertListTops(pagerTop, firstRowTop)

        compose.mainClock.advanceTimeBy(4_001)
        compose.waitForIdle()
        compose.mainClock.advanceTimeBy(DELETION_LINE_FADE_MS.toLong() + 1)
        compose.waitForIdle()
        compose.onNodeWithTag("artist-photo-progress-track").assertDoesNotExist()
        compose.onNodeWithText("6 artists", substring = false).assertIsDisplayed()
        compose.onNodeWithText("6 artists · Artwork 6/6").assertDoesNotExist()
        assertListTops(pagerTop, firstRowTop)
    }

    @Test
    fun failedArtworkSummaryStaysForTenSeconds() {
        val viewModel = MobileSurfaceViewModel()
        showArtists(viewModel)
        compose.runOnIdle { viewModel.openSearch() }
        compose.mainClock.autoAdvance = false
        compose.runOnIdle {
            viewModel.acceptArtistPhotoProgress(
                progress(ArtistPhotoProgressPhase.COMPLETE, done = 4, failed = 2),
            )
        }
        compose.mainClock.advanceTimeByFrame()

        compose.onNodeWithText("6 artists · 2 without a photo").assertIsDisplayed()
        compose.mainClock.advanceTimeBy(4_001)
        compose.waitForIdle()
        compose.onNodeWithText("6 artists · 2 without a photo").assertIsDisplayed()

        compose.mainClock.advanceTimeBy(6_000)
        compose.waitForIdle()
        compose.onNodeWithText("6 artists · 2 without a photo").assertDoesNotExist()
        compose.onNodeWithText("6 artists", substring = false).assertIsDisplayed()
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
        val harness = showArtists(viewModel)
        compose.onNodeWithText(FIRST_ARTIST).performClick()
        compose.onNodeWithTag("artist-detail-overflow").assertIsDisplayed()
        compose.runOnIdle {
            harness.browse.value = harness.browse.value.copy(message = "Browse failed")
        }
        assertStatusBelowPlay("library-status-error", "artist")

        compose.onNodeWithContentDescription("Dismiss").performClick()
        compose.runOnIdle { viewModel.begin("Deleting 13 tracks…") }
        assertStatusBelowPlay("deletion-message-line", "artist")

        assertEquals(
            true,
            libraryStatusDetailInsetApplies(BrowseTab.ARTISTS, detailIsOpen = true),
        )
        assertEquals(
            false,
            libraryStatusDetailInsetApplies(BrowseTab.TITLES, detailIsOpen = true),
        )
    }

    @Test
    fun onAnAlbumDetailPageTheStatusSitsBelowTheHeader() {
        val viewModel = MobileSurfaceViewModel()
        val harness = showArtists(viewModel)
        compose.onNodeWithText(FIRST_ARTIST).performClick()
        compose.onNodeWithTag("artist-detail-overflow").assertIsDisplayed()
        compose.onNodeWithText(FIRST_ALBUM).performClick()
        compose.onNodeWithTag("album-detail-play").assertIsDisplayed()
        compose.runOnIdle {
            harness.browse.value = harness.browse.value.copy(message = "Browse failed")
        }
        assertStatusBelowPlay("library-status-error", "album")

        compose.onNodeWithContentDescription("Dismiss").performClick()
        compose.runOnIdle { viewModel.begin("Deleting 2 tracks…") }
        assertStatusBelowPlay("deletion-message-line", "album")
    }

    @Test
    fun onAnEmptyAlbumDetailPageTheStatusSitsBelowTheNotice() {
        val viewModel = MobileSurfaceViewModel()
        val harness = showArtists(viewModel, emptyAlbum = true)
        compose.onNodeWithText(FIRST_ARTIST).performClick()
        compose.onNodeWithText(FIRST_ALBUM).performClick()
        compose.onNodeWithText("No tracks in this album.").assertIsDisplayed()
        compose.runOnIdle {
            harness.browse.value = harness.browse.value.copy(message = "Browse failed")
        }

        assertStatusBelowElement(
            statusTag = "library-status-error",
            page = "album",
            elementBottom = textBottomInPixels("No tracks in this album."),
        )
    }

    @Test
    fun onAnEmptyArtistDetailPageTheStatusSitsBelowTheNotice() {
        val viewModel = MobileSurfaceViewModel()
        val harness = showArtists(viewModel, emptyArtist = true)
        compose.onNodeWithText(FIRST_ARTIST).performClick()
        compose.onNodeWithText("No tracks by this artist.").assertIsDisplayed()
        compose.runOnIdle {
            harness.browse.value = harness.browse.value.copy(message = "Browse failed")
        }

        assertStatusBelowElement(
            statusTag = "library-status-error",
            page = "artist",
            elementBottom = textBottomInPixels("No tracks by this artist."),
        )
    }

    @Test
    fun onAnArtistPlayFailureTheStatusSitsBelowTheMessage() {
        val viewModel = MobileSurfaceViewModel()
        val harness = showArtists(viewModel, artistTrackIds = { error("catalog unavailable") })
        compose.onNodeWithText(FIRST_ARTIST).performClick()
        compose.onNodeWithTag("artist-detail-play").performClick()
        val failure = "Could not load the tracks: catalog unavailable"
        compose.awaitText(failure)
        compose.runOnIdle {
            harness.browse.value = harness.browse.value.copy(message = "Browse failed")
        }

        assertStatusBelowElement(
            statusTag = "library-status-error",
            page = "artist",
            elementBottom = textBottomInPixels(failure),
        )
    }

    @Test
    fun deletionMessageKeepsItsDismissTimerWhileAnErrorHidesIt() {
        val viewModel = MobileSurfaceViewModel()
        val harness = showArtists(viewModel)
        compose.runOnIdle { viewModel.say("2 tracks deleted") }
        compose.waitForIdle()
        compose.onNodeWithText("2 tracks deleted").assertIsDisplayed()
        compose.runOnIdle {
            harness.browse.value = harness.browse.value.copy(message = "Browse failed")
        }
        compose.waitForIdle()
        compose.onNodeWithText("2 tracks deleted").assertDoesNotExist()

        compose.mainClock.autoAdvance = false
        compose.mainClock.advanceTimeBy(3_000)
        compose.onNodeWithContentDescription("Dismiss").performClick()
        compose.runOnIdle {}
        compose.onNodeWithText("2 tracks deleted", useUnmergedTree = true).assertExists()
        compose.mainClock.advanceTimeBy(1_001L + DELETION_LINE_FADE_MS)
        compose.waitForIdle()

        compose.onNodeWithText("2 tracks deleted").assertDoesNotExist()
    }

    @Test
    fun artworkStopActionMatchesTheCancelablePhasesAndHidesItsChrome() {
        val viewModel = MobileSurfaceViewModel()
        val stops = AtomicInteger()
        showArtists(viewModel)

        compose.onNodeWithContentDescription("Library actions").performClick()
        compose.onNodeWithText("Rescan").assertIsDisplayed()
        compose.onNodeWithText("Stop artwork download").assertDoesNotExist()
        compose.onNodeWithText("Rescan").performClick()

        compose.runOnIdle {
            viewModel.bindArtistPhotoBackfill(
                snapshot = { progress(ArtistPhotoProgressPhase.PREPARING) },
                start = {},
                cancel = { stops.incrementAndGet() },
            )
        }
        assertStopEntryAndCloseMenu()

        compose.runOnIdle {
            viewModel.acceptArtistPhotoProgress(progress(ArtistPhotoProgressPhase.PAUSED, done = 2))
        }
        assertStopEntryAndCloseMenu()

        compose.runOnIdle {
            viewModel.acceptArtistPhotoProgress(progress(ArtistPhotoProgressPhase.RUNNING, done = 2))
        }
        compose.onNodeWithContentDescription("Library actions").performClick()
        compose.onNodeWithText("Stop artwork download").assertIsDisplayed().performClick()
        assertEquals(1, stops.get())
        compose.onNodeWithTag("artist-photo-progress-track").assertDoesNotExist()
        compose.onNodeWithText("6 artists").assertIsDisplayed()

        compose.runOnIdle {
            viewModel.acceptArtistPhotoProgress(progress(ArtistPhotoProgressPhase.COMPLETE, done = 6))
        }
        compose.onNodeWithContentDescription("Library actions").performClick()
        compose.onNodeWithText("Rescan").assertIsDisplayed()
        compose.onNodeWithText("Stop artwork download").assertDoesNotExist()
    }

    private fun showArtists(
        viewModel: MobileSurfaceViewModel,
        openedArtists: AtomicInteger = AtomicInteger(),
        emptyArtist: Boolean = false,
        emptyAlbum: Boolean = false,
        artistTrackIds: (LibraryArtist) -> List<Long> = { emptyList() },
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
                CompositionLocalProvider(LocalArtistTrackIds provides artistTrackIds) {
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
                            ArtistTrackList(
                                artist = it,
                                albums = if (emptyArtist) {
                                    LibraryWindow.empty()
                                } else {
                                    LibraryWindow(1, listOf(album), false)
                                },
                            )
                        },
                        openAlbum = {
                            AlbumTrackList(
                                it,
                                if (emptyAlbum) {
                                    LibraryWindow.empty()
                                } else {
                                    LibraryWindow(1, listOf(albumTrack), false)
                                },
                            )
                        },
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
                    )
                }
            }
        }
        compose.onNodeWithText(FIRST_ARTIST).assertIsDisplayed()
        return Harness(browseState, playbackState)
    }

    private fun assertListTops(pagerTop: Int, firstRowTop: Int) {
        assertEquals(pagerTop, topInPixels("library-destination-pager"))
        assertEquals(firstRowTop, textTopInPixels(FIRST_ARTIST))
    }

    private fun assertStatusBelowPlay(statusTag: String, page: String) {
        assertStatusBelowElement(statusTag, page, bottomInPixels("$page-detail-play"))
    }

    private fun assertStatusBelowElement(statusTag: String, page: String, elementBottom: Int) {
        val statusTop = topInPixels(statusTag)
        assertTrue(statusTop >= elementBottom)
        assertEquals(bottomInPixels("$page-detail-header") + 8f.toPixels(), statusTop)
    }

    private fun assertStopEntryAndCloseMenu() {
        compose.onNodeWithContentDescription("Library actions").performClick()
        compose.onNodeWithText("Rescan").assertIsDisplayed()
        compose.onNodeWithText("Stop artwork download").assertIsDisplayed()
        compose.onNodeWithText("Rescan").performClick()
    }

    private fun topInPixels(tag: String): Int = compose.onNodeWithTag(tag)
        .getUnclippedBoundsInRoot().top.value.toPixels()

    private fun textTopInPixels(text: String): Int = compose.onNodeWithText(text)
        .getUnclippedBoundsInRoot().top.value.toPixels()

    private fun textBottomInPixels(text: String): Int = compose.onNodeWithText(text)
        .getUnclippedBoundsInRoot().bottom.value.toPixels()

    private fun bottomInPixels(tag: String): Int = compose.onNodeWithTag(tag)
        .getUnclippedBoundsInRoot().bottom.value.toPixels()

    private fun Float.toPixels(): Int =
        (this * compose.activity.resources.displayMetrics.density).roundToInt()

    private fun progress(
        phase: ArtistPhotoProgressPhase,
        done: Long = 0,
        failed: Long = 0,
    ) = ArtistPhotoProgress(17, phase, done, failed, 6)

    private val album = LibraryAlbum(
        title = FIRST_ALBUM,
        artist = FIRST_ARTIST,
        representativeUri = "content://albums/first",
        trackCount = 2,
        year = 2026,
        totalDurationMs = 0,
    )

    private val albumTrack = LibraryTrack(
        id = 1,
        uri = "content://albums/first/track",
        title = "Album track",
        artist = FIRST_ARTIST,
        album = FIRST_ALBUM,
        durationMs = 60_000,
        playCount = 0,
        rating = 0,
    )

    private val theme = MobileThemeSelection(
        palette = MobileTheme.NOCTURNE,
        colorScheme = AndroidColorScheme.SYSTEM,
        dynamicAvailable = false,
    )

    private companion object {
        const val FIRST_ARTIST = "Artist 1"
        const val FIRST_ALBUM = "First album"
    }

    private data class Harness(
        val browse: MutableState<LibraryScreenState.Browse>,
        val playback: MutableState<LibraryPlayback>,
    )
}
