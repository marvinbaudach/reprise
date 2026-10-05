package io.github.marvinbaudach.reprise

import androidx.activity.ComponentActivity
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
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

/**
 * Decision 3 of the wave-2 grill: the deletion line never moves the list.
 *
 * `DeletionLineOverlayTest` measures the Artists tab only. This measures a row
 * that is not being deleted on every other place the line can appear — the
 * Titles tab, an artist page, an album page, and the wide short layout — at
 * each moment of a deletion: running, answered, and gone.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36], qualifiers = "w500dp-h1000dp")
class DeletionLineListOffsetTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    fun theTitlesTabStaysWhereItIs() {
        val viewModel = MobileSurfaceViewModel()
        show(viewModel)
        assertDeletionLeavesTheListAlone(viewModel, probeText = SECOND_TITLE)
    }

    @Test
    fun anArtistPageStaysWhereItIs() {
        val viewModel = MobileSurfaceViewModel()
        viewModel.selectTab(BrowseTab.ARTISTS)
        show(viewModel)
        compose.onNodeWithText(ARTIST).performClick()
        compose.onNodeWithText(ALBUM).assertExists()
        assertDeletionLeavesTheListAlone(viewModel, probeText = ALBUM)
    }

    @Test
    fun anAlbumPageStaysWhereItIs() {
        val viewModel = MobileSurfaceViewModel()
        viewModel.selectTab(BrowseTab.ARTISTS)
        show(viewModel)
        compose.onNodeWithText(ARTIST).performClick()
        compose.onNodeWithText(ALBUM).performClick()
        compose.onNodeWithText(SECOND_TITLE).assertExists()
        assertDeletionLeavesTheListAlone(viewModel, probeText = SECOND_TITLE)
    }

    @Test
    @Config(sdk = [36], qualifiers = "w900dp-h400dp")
    fun theWideShortLayoutStaysWhereItIs() {
        val viewModel = MobileSurfaceViewModel()
        show(viewModel)
        assertDeletionLeavesTheListAlone(viewModel, probeText = SECOND_TITLE)
    }

    @Test
    fun theUndoSnackbarFloatsAboveTheBottomFrameWithoutMovingTheList() {
        val harness = DeletionHarness()
        show(harness.surface)
        val before = offsets(SECOND_TITLE)

        compose.runOnIdle {
            harness.surface.pendingDeletions.offers.show("Removed from queue", onUndo = {})
        }
        compose.waitUntil(WAIT_MS) {
            compose.onAllNodesWithText("Removed from queue").fetchSemanticsNodes().isNotEmpty()
        }
        compose.waitForIdle()

        assertEquals("while offered", before, offsets(SECOND_TITLE))
        val snackbarBottom = compose.onNodeWithTag("undo-snackbar-host")
            .getUnclippedBoundsInRoot().bottom.value.toPixels()
        val frameTop = topOfTag("library-navigation-bar")
        assertTrue("snackbar ends at $snackbarBottom, frame starts at $frameTop", snackbarBottom <= frameTop)

        compose.runOnIdle { harness.passTheWindow() }
        compose.waitForIdle()
        assertEquals("once gone", before, offsets(SECOND_TITLE))
    }

    private fun assertDeletionLeavesTheListAlone(
        viewModel: MobileSurfaceViewModel,
        probeText: String,
    ) {
        val before = Offsets(topOfTag("library-destination-pager"), topOfText(probeText))
        lateinit var run: DeletionRun
        compose.runOnIdle { run = viewModel.begin("Deleting 2 tracks…") }
        compose.waitForIdle()
        assertEquals("while running", before, offsets(probeText))

        compose.runOnIdle { run.finish("2 tracks deleted") }
        compose.waitForIdle()
        assertEquals("once answered", before, offsets(probeText))

        compose.mainClock.advanceTimeBy(TRANSIENT_MESSAGE_MS + 1)
        compose.waitForIdle()
        compose.mainClock.advanceTimeBy(DELETION_LINE_FADE_MS.toLong() + 1)
        compose.waitForIdle()
        assertEquals("once gone", before, offsets(probeText))
    }

    private fun show(viewModel: MobileSurfaceViewModel) {
        val titles = LibraryWindow(
            total = 4,
            rows = listOf(FIRST_TITLE, SECOND_TITLE, "Third", "Fourth")
                .mapIndexed { index, title -> configurationTestTrack(index + 1L, title) },
            hasMore = false,
        )
        val artist = LibraryArtist(ARTIST, 4, 1, "content://artists/1")
        val artists = LibraryWindow(total = 1, rows = listOf(artist), hasMore = false)
        val album = LibraryAlbum(
            title = ALBUM,
            artist = ARTIST,
            representativeUri = "content://albums/1",
            trackCount = 4,
            year = 2026,
            totalDurationMs = 0,
        )
        compose.setContent {
            RepriseTheme(theme, darkPalette = true) {
                BrowseScreen(
                    state = LibraryScreenState.Browse(titles = titles, artists = artists),
                    playback = PlaybackUiState().libraryPlayback(),
                    playbackSettingsRevision = 0,
                    surfaceState = viewModel,
                    chooseFolder = {},
                    rescan = {},
                    searchTitles = { _, _ -> titles },
                    listArtists = { artists },
                    openArtist = {
                        ArtistTrackList(
                            artist = it,
                            albums = LibraryWindow(1, listOf(album), false),
                        )
                    },
                    openAlbum = { AlbumTrackList(it, titles) },
                    listAlbumTracks = { _, _ -> titles },
                    loadTrack = { _, deliver -> deliver(null) },
                    playTracks = { _, _ -> },
                    loadPlaybackSettings = {
                        PlaybackSettingsUiState(false, true, emptyList())
                    },
                    setEqualizerEnabled = { PlaybackSettingsUiState(it, true, emptyList()) },
                    replaceEqualizerCurve = { PlaybackSettingsUiState(false, true, emptyList()) },
                    setGaplessEnabled = { PlaybackSettingsUiState(false, it, emptyList()) },
                    themeSelection = theme,
                    selectTheme = {},
                )
            }
        }
    }

    private data class Offsets(val pagerTop: Int, val rowTop: Int)

    private fun offsets(probeText: String) =
        Offsets(topOfTag("library-destination-pager"), topOfText(probeText))

    private fun topOfTag(tag: String) = compose.onNodeWithTag(tag)
        .getUnclippedBoundsInRoot().top.value.toPixels()

    private fun topOfText(text: String) = compose.onNodeWithText(text)
        .getUnclippedBoundsInRoot().top.value.toPixels()

    private fun Float.toPixels(): Int =
        (this * compose.activity.resources.displayMetrics.density).roundToInt()

    private val theme = MobileThemeSelection(
        palette = MobileTheme.NOCTURNE,
        colorScheme = AndroidColorScheme.SYSTEM,
        dynamicAvailable = false,
    )

    private companion object {
        const val WAIT_MS = 5_000L
        const val FIRST_TITLE = "First song"
        const val SECOND_TITLE = "Second song"
        const val ARTIST = "Only artist"
        const val ALBUM = "Only album"
    }
}
