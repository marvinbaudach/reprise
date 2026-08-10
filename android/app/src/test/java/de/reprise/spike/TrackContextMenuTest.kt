package de.reprise.spike

import androidx.activity.ComponentActivity
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.longClick
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTouchInput
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import uniffi.reprise_android_ffi.AndroidRepeatMode
import uniffi.reprise_android_ffi.AndroidTrashReport

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36], qualifiers = "w412dp-h916dp-port")
class TrackContextMenuTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    fun longPressingATitleRowCanEnqueueItWithoutStartingPlayback() {
        val controls = RecordingContextMenuControls()
        var playCount = 0
        val track = configurationTestTrack(41, "Menu Song")

        compose.setContent {
            MaterialTheme {
                CompositionLocalProvider(LocalPlaybackControls provides controls) {
                    TrackRows(
                        surfaceLayout = SurfaceLayout.STACKED,
                        surfaceState = MobileSurfaceViewModel(),
                        listKey = LibraryListKey.TITLES,
                        tracks = LibraryWindow(total = 1, rows = listOf(track), hasMore = false),
                        playback = PlaybackUiState(),
                        lastRequestedOffset = null,
                        play = { playCount += 1 },
                        loadMore = {},
                    )
                }
            }
        }

        compose.onNodeWithTag("library-track-row-41").performTouchInput { longClick() }
        compose.onNodeWithText("Play next").performClick()

        assertEquals(listOf(listOf(41L)), controls.queuedNext)
        assertEquals(0, playCount)
    }

    @Test
    fun deletingAlwaysConfirmsAndCancelLeavesTheTrackUntouched() {
        val controls = RecordingContextMenuControls()
        val track = configurationTestTrack(41, "Menu Song")
        composeTitleRow(track, controls)

        openTitleMenu(track.id)
        compose.onNodeWithText("Delete from device…").performClick()
        compose.onNodeWithText("Delete Menu Song?").assertIsDisplayed()
        compose.onNodeWithText("This cannot be undone.", substring = true).assertIsDisplayed()
        compose.onNodeWithText("Cancel").performClick()
        compose.onNodeWithText("Delete Menu Song?").assertDoesNotExist()
        assertEquals(emptyList<List<Long>>(), controls.deleted)

        openTitleMenu(track.id)
        compose.onNodeWithText("Delete from device…").performClick()
        compose.onNodeWithText("Delete", useUnmergedTree = true).performClick()
        assertEquals(listOf(listOf(41L)), controls.deleted)
    }

    @Test
    fun longPressingAnAlbumPlaysEveryUnwindowedIdInOrder() {
        val controls = RecordingContextMenuControls()
        val album = LibraryAlbum(
            title = "Whole Album",
            artist = "Album Artist",
            representativeUri = "content://albums/whole",
            trackCount = 3,
            year = 2026,
            totalDurationMs = 360_000,
        )
        compose.setContent {
            MaterialTheme {
                CompositionLocalProvider(
                    LocalPlaybackControls provides controls,
                    LocalAlbumTrackIds provides { listOf(9L, 7L, 5L) },
                ) {
                    AlbumsTab(
                        surfaceLayout = SurfaceLayout.STACKED,
                        surfaceState = MobileSurfaceViewModel(),
                        albums = LibraryWindow(1, listOf(album), false),
                        selectedAlbum = null,
                        playback = PlaybackUiState(),
                        openAlbum = {},
                        closeAlbum = {},
                        play = {},
                        albumsRequestedOffset = null,
                        albumRequestedOffset = null,
                        loadMoreAlbums = {},
                        loadMoreAlbumTracks = {},
                    )
                }
            }
        }

        compose.onNodeWithText("Whole Album").performTouchInput { longClick() }
        compose.onNodeWithText("Play").performClick()

        assertEquals(listOf(9L, 7L, 5L), controls.playedIds)
        assertEquals(0, controls.playedStartIndex)
    }

    @Test
    fun longPressingAQueueRowMovesExactlyOnePosition() {
        val tracks = listOf(
            configurationTestTrack(41, "Queued One"),
            configurationTestTrack(42, "Queued Two"),
        )
        val controls = RecordingContextMenuControls(tracks)
        compose.setContent {
            MaterialTheme {
                CompositionLocalProvider(LocalPlaybackControls provides controls) {
                    NowPlayingQueuePage(
                        PlaybackUiState(),
                        MobileSurfaceViewModel(),
                        SurfaceLayout.STACKED,
                    )
                }
            }
        }

        compose.onNodeWithTag("queue-track-row-41").performTouchInput { longClick() }
        compose.onNodeWithText("Play now").assertIsDisplayed()
        compose.onNodeWithText("Move up").assertIsDisplayed()
        compose.onNodeWithText("Move down").performClick()

        assertEquals(listOf(Triple(0, 41L, 1)), controls.moved)
    }

    @Test
    fun stackedNowPlayingActionsExposeTheSingleDeleteMenu() {
        composeNowPlayingMenu(SurfaceLayout.STACKED)

        compose.onNodeWithTag("now-playing-overflow").performClick()
        compose.onNodeWithText("Delete from device…").assertIsDisplayed()
    }

    @Test
    fun wideNowPlayingActionsExposeTheSingleDeleteMenu() {
        RuntimeEnvironment.setQualifiers("w916dp-h412dp-land")
        compose.activityRule.scenario.recreate()
        composeNowPlayingMenu(SurfaceLayout.WIDE_SHORT)

        compose.onNodeWithTag("now-playing-overflow").performClick()
        compose.onNodeWithText("Delete from device…").assertIsDisplayed()
    }

    private fun composeNowPlayingMenu(layout: SurfaceLayout) {
        val track = configurationTestTrack(41, "Now Playing Menu Song")
        compose.setContent {
            MaterialTheme {
                CompositionLocalProvider(
                    LocalPlaybackControls provides RecordingContextMenuControls(),
                ) {
                    NowPlayingSheet(
                        track = track,
                        playback = PlaybackUiState(currentTrackId = track.id),
                        surfaceLayout = layout,
                        surfaceState = MobileSurfaceViewModel(),
                        close = {},
                    )
                }
            }
        }
    }

    private fun composeTitleRow(
        track: LibraryTrack,
        controls: RecordingContextMenuControls,
    ) {
        compose.setContent {
            MaterialTheme {
                CompositionLocalProvider(LocalPlaybackControls provides controls) {
                    TrackRows(
                        surfaceLayout = SurfaceLayout.STACKED,
                        surfaceState = MobileSurfaceViewModel(),
                        listKey = LibraryListKey.TITLES,
                        tracks = LibraryWindow(total = 1, rows = listOf(track), hasMore = false),
                        playback = PlaybackUiState(),
                        lastRequestedOffset = null,
                        play = {},
                        loadMore = {},
                    )
                }
            }
        }
    }

    private fun openTitleMenu(trackId: Long) {
        compose.onNodeWithTag("library-track-row-$trackId").performTouchInput { longClick() }
    }
}

private class RecordingContextMenuControls(
    private val upcoming: List<LibraryTrack> = emptyList(),
) : PlaybackControls {
    var playedIds: List<Long>? = null
    var playedStartIndex: Int? = null
    val queuedNext = mutableListOf<List<Long>>()
    val deleted = mutableListOf<List<Long>>()
    val moved = mutableListOf<Triple<Int, Long, Int>>()

    override fun togglePause() = Unit
    override fun next() = Unit
    override fun previous() = Unit
    override fun seekTo(positionMs: Long) = Unit
    override fun setShuffle(enabled: Boolean) = Unit
    override fun setRepeat(mode: AndroidRepeatMode) = Unit
    override fun setFavourite(
        trackId: Long,
        favourite: Boolean,
        report: (String?) -> Unit,
    ) = report(null)

    override fun playTrackIds(trackIds: List<Long>, startIndex: Int) {
        playedIds = trackIds
        playedStartIndex = startIndex
    }

    override fun loadUpcomingTracks(
        window: LibraryWindowRange,
        report: (Result<LibraryWindow<LibraryTrack>>) -> Unit,
    ) = report(
        Result.success(
            LibraryWindow(
                total = upcoming.size.toLong(),
                rows = upcoming.drop(window.offset.toInt()).take(window.limit.toInt()),
                hasMore = false,
            ),
        ),
    )

    override fun moveUpcomingTrack(
        fromPosition: Int,
        expectedTrackId: Long,
        toPosition: Int,
        report: (Result<Boolean>) -> Unit,
    ) {
        moved += Triple(fromPosition, expectedTrackId, toPosition)
        report(Result.success(true))
    }

    override fun queueTracksNext(trackIds: List<Long>, report: (Result<UInt>) -> Unit) {
        queuedNext += trackIds
        report(Result.success(trackIds.size.toUInt()))
    }

    override fun deleteTracks(
        trackIds: List<Long>,
        report: (Result<AndroidTrashReport>) -> Unit,
    ) {
        deleted += trackIds
        report(Result.success(AndroidTrashReport(removedIds = trackIds, failures = emptyList())))
    }
}
