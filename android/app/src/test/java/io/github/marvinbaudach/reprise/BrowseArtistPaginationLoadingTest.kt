package io.github.marvinbaudach.reprise

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithContentDescription
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollToIndex
import java.util.concurrent.CountDownLatch
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.withContext
import org.junit.After
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(
    sdk = [36],
    qualifiers = "w412dp-h916dp-port",
    application = BrowseArtistPaginationTestApplication::class,
)
class BrowseArtistPaginationLoadingTest {
    @get:Rule
    val compose = createAndroidComposeRule<MainActivity>()

    private val application: BrowseArtistPaginationTestApplication
        get() = RuntimeEnvironment.getApplication() as BrowseArtistPaginationTestApplication

    @After
    fun releaseTheService() {
        application.releasePagination()
        application.releaseService()
    }

    @Test
    fun aLateArtistAlbumPageDoesNotReopenTheArtistAfterSystemBack() {
        application.blockAlbumPagination()
        openArtistOne()
        startPagination()

        compose.runOnIdle {
            compose.activity.onBackPressedDispatcher.onBackPressed()
            application.releasePagination()
        }
        compose.waitUntil(timeoutMillis = 5_000) { application.paginationHasFinished() }
        compose.waitForIdle()

        assertArtistListStayedOpen()
    }

    @Test
    fun aLateArtistTrackPageDoesNotReopenTheArtistAfterSystemBack() {
        application.blockTrackPagination()
        openArtistOne()
        startPagination()

        compose.runOnIdle {
            compose.activity.onBackPressedDispatcher.onBackPressed()
            application.releasePagination()
        }
        compose.waitUntil(timeoutMillis = 5_000) { application.paginationHasFinished() }
        compose.waitForIdle()

        assertArtistListStayedOpen()
    }

    private fun openArtistOne() {
        compose.onNodeWithText("Artists").performClick()
        compose.onAllNodesWithText("Artist 1")[0].performClick()
        compose.waitUntil(timeoutMillis = 5_000) {
            compose.onAllNodesWithContentDescription("Play Artist 1")
                .fetchSemanticsNodes().isNotEmpty()
        }
    }

    private fun startPagination() {
        compose.onNodeWithTag("library-artist-albums-list").performScrollToIndex(202)
        compose.waitUntil(timeoutMillis = 5_000) { application.paginationHasStarted() }
    }

    private fun assertArtistListStayedOpen() {
        compose.onNodeWithContentDescription("Back to artists").assertDoesNotExist()
        compose.onNodeWithText("Albums").assertDoesNotExist()
        compose.onNodeWithText("Other titles").assertDoesNotExist()
        compose.onNodeWithText("Artist 2").assertIsDisplayed()
    }
}

internal class BrowseArtistPaginationTestApplication : ConfigurationTestApplication() {
    private enum class PaginationKind { ALBUMS, TRACKS }

    private val albums = (1..201).map { index ->
        LibraryAlbum(
            title = "Paged Album $index",
            artist = "Artist 1",
            representativeUri = "content://provider/paged-album/$index.flac",
            trackCount = 1,
            year = 2026,
            totalDurationMs = 120_000,
        )
    }
    private val tracks = (1..201).map { index ->
        configurationTestTrack(3_000_000L + index, "Paged Artist Track $index")
    }
    private var paginationKind = PaginationKind.ALBUMS
    private var started = CountDownLatch(1)
    private var gate = CompletableDeferred<Unit>()
    private var finished = CountDownLatch(1)

    fun blockAlbumPagination() = blockPagination(PaginationKind.ALBUMS)

    fun blockTrackPagination() = blockPagination(PaginationKind.TRACKS)

    private fun blockPagination(kind: PaginationKind) {
        paginationKind = kind
        started = CountDownLatch(1)
        gate = CompletableDeferred()
        finished = CountDownLatch(1)
    }

    fun paginationHasStarted(): Boolean = started.count == 0L

    fun releasePagination() {
        gate.complete(Unit)
    }

    fun paginationHasFinished(): Boolean = finished.count == 0L

    override fun mainActivitySurface(): MainActivitySurfaceDependencies {
        val dependencies = super.mainActivitySurface()
        return dependencies.copy(
            openArtist = { artist ->
                ArtistTrackList(
                    artist = artist,
                    albums = if (paginationKind == PaginationKind.ALBUMS) {
                        albums.testWindow(firstLibraryWindow())
                    } else {
                        LibraryWindow.empty()
                    },
                    untaggedTracks = if (paginationKind == PaginationKind.TRACKS) {
                        tracks.testWindow(firstLibraryWindow())
                    } else {
                        LibraryWindow.empty()
                    },
                )
            },
            listArtistAlbums = { _, range ->
                blockContinuation(PaginationKind.ALBUMS, range)
                albums.testWindow(range)
            },
            listArtistUntaggedTracks = { _, range ->
                blockContinuation(PaginationKind.TRACKS, range)
                tracks.testWindow(range)
            },
        )
    }

    private suspend fun blockContinuation(kind: PaginationKind, range: LibraryWindowRange) {
        if (paginationKind != kind || range.offset == 0L) return
        started.countDown()
        withContext(NonCancellable) {
            gate.await()
            finished.countDown()
        }
    }
}

private fun <T> List<T>.testWindow(range: LibraryWindowRange): LibraryWindow<T> {
    val from = range.offset.toInt().coerceIn(0, size)
    val until = (from + range.limit.toInt()).coerceIn(from, size)
    return LibraryWindow(
        total = size.toLong(),
        rows = subList(from, until).toList(),
        hasMore = until < size,
    )
}
