package io.github.marvinbaudach.reprise

import androidx.activity.ComponentActivity
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.click
import io.github.marvinbaudach.reprise.ui.theme.RepriseTheme
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import kotlin.math.roundToInt
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.reprise_android_ffi.AndroidColorScheme

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36], qualifiers = "w500dp-h1000dp")
class DeletionLineOverlayTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    fun aDeletionLineDoesNotMoveTheListUnderTheFinger() {
        val viewModel = MobileSurfaceViewModel()
        showArtists(viewModel)
        val pagerTop = topInPixels("library-destination-pager")
        val firstRowTop = textTopInPixels(FIRST_ARTIST)

        lateinit var deletion: DeletionRun
        compose.runOnIdle { deletion = viewModel.begin("Deleting 13 tracks…") }
        compose.waitForIdle()

        compose.onNodeWithTag("deletion-message-line").assertIsDisplayed()
        assertEquals(pagerTop, topInPixels("library-destination-pager"))
        assertEquals(firstRowTop, textTopInPixels(FIRST_ARTIST))

        compose.runOnIdle { deletion.finish("13 tracks deleted") }
        compose.waitForIdle()
        compose.mainClock.advanceTimeBy(TRANSIENT_MESSAGE_MS + 1)
        compose.waitForIdle()
        compose.mainClock.advanceTimeBy(DELETION_LINE_FADE_MS.toLong() + 1)
        compose.waitForIdle()

        compose.onNodeWithTag("deletion-message-line").assertDoesNotExist()
        assertEquals(pagerTop, topInPixels("library-destination-pager"))
        assertEquals(firstRowTop, textTopInPixels(FIRST_ARTIST))
    }

    @Test
    fun aTapOnTheDeletionLineReachesTheRowUnderneath() {
        val openedArtists = AtomicInteger()
        val viewModel = MobileSurfaceViewModel()
        showArtists(viewModel, openedArtists)

        compose.runOnIdle { viewModel.begin("Deleting 13 tracks…") }
        compose.waitForIdle()
        compose.onNodeWithTag("deletion-message-line")
            .assertIsDisplayed()
            .performTouchInput { click(center) }

        compose.waitUntil { openedArtists.get() == 1 }
        assertEquals(1, openedArtists.get())
    }

    /**
     * Coverage test: the pre-overlay line could already render this path, so
     * this test is allowed to be green from its first run.
     */
    @Test
    fun aListThatChangedWhileResolvingIsSaidOnTheScreen() {
        val started = CountDownLatch(1)
        val gate = CountDownLatch(1)
        val viewModel = MobileSurfaceViewModel()
        var present by mutableStateOf(true)
        val anchor = TrackContextMenuAnchorState()
        val target = LibraryTrackMenuTarget(
            label = "Whole Artist",
            trackCount = 3,
            resolveTrackIds = {
                started.countDown()
                gate.await(GATE_MS, TimeUnit.MILLISECONDS)
                listOf(9L, 7L, 5L)
            },
            play = {},
        )
        compose.setContent {
            MaterialTheme {
                CompositionLocalProvider(
                    LocalPlaybackControls provides RecordingContextMenuControls(),
                    LocalDeletionMessages provides viewModel,
                ) {
                    DeletionMessageLine(viewModel)
                    if (present) TrackContextMenu(anchor = anchor, target = target)
                }
            }
        }
        compose.runOnIdle { anchor.expanded = true }
        compose.onNodeWithText("Delete from device…").performClick()
        assertTrue(started.await(GATE_MS, TimeUnit.MILLISECONDS))

        compose.runOnIdle { present = false }
        compose.waitForIdle()
        gate.countDown()
        compose.waitUntil(WAIT_MS) {
            viewModel.deletionMessage?.text == LIST_CHANGED_MESSAGE
        }

        compose.onNode(
            hasText(LIST_CHANGED_MESSAGE) and
                hasAnyAncestor(hasTestTag("deletion-message-line")),
        ).assertIsDisplayed()
    }

    private fun showArtists(
        viewModel: MobileSurfaceViewModel,
        openedArtists: AtomicInteger = AtomicInteger(),
    ) {
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
        val browse = LibraryScreenState.Browse(
            titles = LibraryWindow.empty(),
            artists = artistWindow,
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
                    listArtists = { artistWindow },
                    openArtist = { artist ->
                        openedArtists.incrementAndGet()
                        ArtistTrackList(artist = artist)
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
                )
            }
        }
        compose.onNodeWithText(FIRST_ARTIST).assertIsDisplayed()
    }

    private fun topInPixels(tag: String): Int = compose.onNodeWithTag(tag)
        .getUnclippedBoundsInRoot()
        .top
        .value
        .toPixels()

    private fun textTopInPixels(text: String): Int = compose.onNodeWithText(text)
        .getUnclippedBoundsInRoot()
        .top
        .value
        .toPixels()

    private fun Float.toPixels(): Int =
        (this * compose.activity.resources.displayMetrics.density).roundToInt()

    private val theme = MobileThemeSelection(
        palette = MobileTheme.NOCTURNE,
        colorScheme = AndroidColorScheme.SYSTEM,
        dynamicAvailable = false,
    )

    private companion object {
        const val FIRST_ARTIST = "Artist 1"
        const val GATE_MS = 5_000L
        const val WAIT_MS = 5_000L
        const val LIST_CHANGED_MESSAGE =
            "The list changed before the tracks of Whole Artist were found. Nothing was done."
    }
}
