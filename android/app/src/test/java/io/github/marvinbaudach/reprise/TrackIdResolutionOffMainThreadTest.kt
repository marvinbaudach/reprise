package io.github.marvinbaudach.reprise

import android.os.Looper
import androidx.activity.ComponentActivity
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNotSame
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * "Delete from device…", Play, Play next, Add to queue and the artist page's
 * Play ask the catalog for every id of an album or artist. That query is
 * unwindowed — a big artist is thousands of rows — so it must not run on the
 * thread that draws the menu. These tests hold the resolver to a background
 * thread and hold the menu to what it owes while the answer is pending.
 *
 * Every gate is bounded: on a menu that resolves on the main thread the gate
 * would otherwise block the test itself instead of failing it.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36], qualifiers = "w412dp-h916dp-port")
class TrackIdResolutionOffMainThreadTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private val deletionSays = mutableListOf<String>()
    private val artist = LibraryArtist("Whole Artist", 3, 2, "content://artists/whole")
    private val otherArtist = LibraryArtist("Other Artist", 1, 1, "content://artists/other")
    private var shownArtist by mutableStateOf(artist)
    private lateinit var anchor: TrackContextMenuAnchorState

    private val timers = ManualTimers()
    private val deletionMessages = object : DeletionMessages {
        override val pendingDeletions = PendingDeletions(
            offers = UndoOffers(timers::schedule),
            messages = this,
            currentTrackId = { null },
            latestRefreshTicket = { 0 },
        )

        override fun say(text: String) {
            deletionSays += text
        }

        override fun begin(text: String): DeletionRun {
            say(text)
            return DeletionRun { outcome -> say(outcome) }
        }
    }

    @Test
    fun deletingResolvesTheIdsOffTheMainThreadAndThenOffersTheUndo() {
        val resolvedOn = AtomicReference<Thread>()
        showMenu(RecordingContextMenuControls()) {
            resolvedOn.set(Thread.currentThread())
            listOf(9L, 7L, 5L)
        }

        compose.onNodeWithText("Delete from device…").performClick()
        awaitUndoOffer()

        assertNotNull("the resolver never ran", resolvedOn.get())
        assertNotSame(Looper.getMainLooper().thread, resolvedOn.get())
    }

    @Test
    fun playPlayNextAndAddToQueueResolveOffTheMainThreadToo() {
        val resolvedOn = mutableListOf<Thread>()
        val controls = RecordingContextMenuControls()
        showMenu(controls) {
            synchronized(resolvedOn) { resolvedOn += Thread.currentThread() }
            listOf(9L, 7L, 5L)
        }

        compose.onNode(menuItem("Play")).performClick()
        compose.waitUntil(WAIT_MS) { controls.playedIds != null }
        reopenMenu()
        compose.onNode(menuItem("Play next")).performClick()
        compose.waitUntil(WAIT_MS) { controls.queuedNext.isNotEmpty() }
        reopenMenu()
        compose.onNode(menuItem("Add to queue")).performClick()
        compose.waitUntil(WAIT_MS) { controls.queuedLast.isNotEmpty() }

        assertEquals(listOf(9L, 7L, 5L), controls.playedIds)
        assertEquals(listOf(listOf(9L, 7L, 5L)), controls.queuedNext)
        assertEquals(listOf(listOf(9L, 7L, 5L)), controls.queuedLast)
        synchronized(resolvedOn) {
            assertEquals(3, resolvedOn.size)
            resolvedOn.forEach { assertNotSame(Looper.getMainLooper().thread, it) }
        }
    }

    @Test
    fun noUndoIsOfferedUntilTheResolutionHasAnswered() {
        val gate = CountDownLatch(1)
        showMenu(RecordingContextMenuControls()) {
            gate.await(GATE_MS, TimeUnit.MILLISECONDS)
            listOf(9L, 7L, 5L)
        }

        compose.onNodeWithText("Delete from device…").performClick()
        compose.waitForIdle()
        assertNull(deletionMessages.pendingDeletions.offers.current)

        gate.countDown()
        awaitUndoOffer()
    }

    @Test
    fun aSecondDeleteWhileTheFirstIsResolvingIsNotOffered() {
        val gate = CountDownLatch(1)
        val resolves = AtomicInteger()
        showMenu(RecordingContextMenuControls()) {
            resolves.incrementAndGet()
            gate.await(GATE_MS, TimeUnit.MILLISECONDS)
            listOf(9L, 7L, 5L)
        }

        compose.onNodeWithText("Delete from device…").performClick()
        compose.waitForIdle()
        reopenMenu()
        compose.onNodeWithText("Delete from device…").assertIsNotEnabled()
        compose.onNodeWithText("Delete from device…").performClick()

        gate.countDown()
        awaitUndoOffer()
        compose.waitForIdle()

        assertEquals("the target was resolved once", 1, resolves.get())
        assertEquals("one delete was offered", 1, timers.delays.size)
    }

    @Test
    fun aFailedResolutionSaysWhyThroughTheDeletionLineAndCanBeRetried() {
        val attempts = AtomicInteger()
        showMenu(RecordingContextMenuControls()) {
            if (attempts.incrementAndGet() == 1) error("catalog unavailable")
            listOf(9L, 7L, 5L)
        }

        compose.onNodeWithText("Delete from device…").performClick()
        compose.waitUntil(WAIT_MS) { deletionSays.isNotEmpty() }
        compose.waitForIdle()

        assertEquals(listOf("Could not load the tracks: catalog unavailable"), deletionSays)
        assertNull(deletionMessages.pendingDeletions.offers.current)

        reopenMenu()
        compose.onNodeWithText("Delete from device…").assertIsEnabled()
        compose.onNodeWithText("Delete from device…").performClick()
        awaitUndoOffer()
    }

    @Test
    fun aRowThatLeavesWhileResolvingSaysOnTheScreenLineThatNothingWasDone() {
        val started = CountDownLatch(1)
        val gate = CountDownLatch(1)
        var present by mutableStateOf(true)
        val leavingAnchor = TrackContextMenuAnchorState()
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
                    LocalDeletionMessages provides deletionMessages,
                ) {
                    if (present) {
                        TrackContextMenu(anchor = leavingAnchor, target = target)
                    }
                }
            }
        }
        compose.runOnIdle { leavingAnchor.expanded = true }
        compose.onNodeWithText("Delete from device…").performClick()
        assertTrue(started.await(GATE_MS, TimeUnit.MILLISECONDS))

        compose.runOnIdle { present = false }
        compose.waitForIdle()
        gate.countDown()
        compose.waitUntil(WAIT_MS) { deletionSays.isNotEmpty() }

        assertEquals(
            listOf(
                "The list changed before the tracks of Whole Artist were found. Nothing was done.",
            ),
            deletionSays,
        )
    }

    @Test
    fun theArtistPagePlayButtonResolvesOffTheMainThreadThenPlays() {
        val resolvedOn = AtomicReference<Thread>()
        val gate = CountDownLatch(1)
        val controls = RecordingContextMenuControls()
        showArtistPage(controls) {
            resolvedOn.set(Thread.currentThread())
            gate.await(GATE_MS, TimeUnit.MILLISECONDS)
            listOf(9L, 7L, 5L)
        }

        compose.onNodeWithContentDescription("Play Whole Artist").performClick()
        compose.waitForIdle()
        assertNull("nothing plays before the ids are known", controls.playedIds)

        gate.countDown()
        compose.waitUntil(WAIT_MS) { controls.playedIds != null }

        assertEquals(listOf(9L, 7L, 5L), controls.playedIds)
        assertNotSame(Looper.getMainLooper().thread, resolvedOn.get())
    }

    @Test
    fun anArtistPagePlayStillResolvingWhenThePageShowsAnotherArtistIsDropped() {
        val started = CountDownLatch(1)
        val gate = CountDownLatch(1)
        val controls = RecordingContextMenuControls()
        showArtistPage(controls) {
            started.countDown()
            gate.await(GATE_MS, TimeUnit.MILLISECONDS)
            listOf(9L, 7L, 5L)
        }

        compose.onNodeWithContentDescription("Play Whole Artist").performClick()
        assertTrue(started.await(GATE_MS, TimeUnit.MILLISECONDS))
        compose.runOnIdle { shownArtist = otherArtist }
        compose.onNodeWithContentDescription("Play Other Artist").assertIsEnabled()

        gate.countDown()
        Thread.sleep(SETTLE_MS)
        compose.waitForIdle()

        assertNull("the page no longer shows the artist that was asked for", controls.playedIds)
    }

    @Test
    fun aRefreshThatOnlyRecountsTheArtistLetsItsPlayFinish() {
        val started = CountDownLatch(1)
        val gate = CountDownLatch(1)
        val controls = RecordingContextMenuControls()
        showArtistPage(controls) {
            started.countDown()
            gate.await(GATE_MS, TimeUnit.MILLISECONDS)
            listOf(9L, 7L, 5L)
        }

        compose.onNodeWithContentDescription("Play Whole Artist").performClick()
        assertTrue(started.await(GATE_MS, TimeUnit.MILLISECONDS))
        compose.runOnIdle { shownArtist = artist.copy(trackCount = 2) }
        compose.waitForIdle()

        gate.countDown()
        compose.waitUntil(WAIT_MS) { controls.playedIds != null }

        assertEquals(listOf(9L, 7L, 5L), controls.playedIds)
    }

    @Test
    fun aFailedArtistPagePlaySaysWhyInsteadOfCrashing() {
        val controls = RecordingContextMenuControls()
        showArtistPage(controls) { error("catalog unavailable") }

        compose.onNodeWithContentDescription("Play Whole Artist").performClick()
        compose.awaitText("Could not load the tracks: catalog unavailable")

        assertNull(controls.playedIds)
    }

    private fun menuItem(label: String) =
        hasText(label) and hasAnyAncestor(hasTestTag("library-track-context-menu"))

    private fun awaitUndoOffer() {
        compose.waitUntil(WAIT_MS) { deletionMessages.pendingDeletions.offers.current != null }
    }

    private fun reopenMenu() {
        compose.runOnIdle { anchor.expanded = true }
    }

    private fun showMenu(
        controls: RecordingContextMenuControls,
        resolve: () -> List<Long>,
    ) {
        anchor = TrackContextMenuAnchorState()
        val target = LibraryTrackMenuTarget(
            label = "Whole Artist",
            trackCount = 3,
            resolveTrackIds = resolve,
            play = { ids -> controls.playTrackIds(ids, 0) },
        )
        compose.setContent {
            MaterialTheme {
                CompositionLocalProvider(
                    LocalPlaybackControls provides controls,
                    LocalDeletionMessages provides deletionMessages,
                ) {
                    Column {
                        TrackContextMenu(anchor = anchor, target = target)
                        TrackContextMenuMessage(anchor)
                    }
                }
            }
        }
        reopenMenu()
    }

    private fun showArtistPage(
        controls: RecordingContextMenuControls,
        resolve: () -> List<Long>,
    ) {
        val surfaceState = MobileSurfaceViewModel()
        val album = LibraryAlbum("First", artist.name, "content://albums/first", 2, 2026, 0)
        compose.setContent {
            MaterialTheme {
                CompositionLocalProvider(
                    LocalPlaybackControls provides controls,
                    LocalAlbumTrackIds provides { listOf(4L) },
                    LocalArtistTrackIds provides { resolve() },
                ) {
                    ArtistsTab(
                        surfaceLayout = SurfaceLayout.STACKED,
                        surfaceState = surfaceState,
                        artists = LibraryWindow(1, listOf(artist), false),
                        searchText = "",
                        selectedArtist = ArtistTrackList(
                            artist = shownArtist,
                            albums = LibraryWindow(1, listOf(album), false),
                        ),
                        playback = PlaybackUiState().libraryPlayback(),
                        openArtist = {},
                        openAlbum = {},
                        closeArtist = {},
                        play = {},
                        lastRequestedOffset = null,
                        artistRequestedOffset = null,
                        loadMoreArtists = {},
                        loadMoreArtistTracks = {},
                    )
                }
            }
        }
    }

    private companion object {
        const val WAIT_MS = AWAIT_TIMEOUT_MS

        /** Longer than any wait above, so a gate never releases a test by timing out. */
        const val GATE_MS = 10_000L

        /** Time for a wrongly delivered answer to reach the main thread. */
        const val SETTLE_MS = 300L
    }
}
