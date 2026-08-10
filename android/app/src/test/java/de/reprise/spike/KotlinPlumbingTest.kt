package de.reprise.spike

import android.content.Context
import androidx.test.core.app.ApplicationProvider
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.reprise_android_ffi.AndroidTrashFailure
import uniffi.reprise_android_ffi.AndroidTrashReport
import uniffi.reprise_android_ffi.MusicLibrary
import uniffi.reprise_android_ffi.NoHandle
import uniffi.reprise_android_ffi.TrashAction

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class KotlinPlumbingTest {
    @Test
    fun albumIdsCrossTheLibraryPortWithoutPagingOrReordering() {
        val context = ApplicationProvider.getApplicationContext<Context>()
        val library = RecordingAlbumLibrary()
        val session = LibrarySession(
            AndroidLibrarySessionPort(
                resolver = context.contentResolver,
                preferences = context.getSharedPreferences("plumbing-test", Context.MODE_PRIVATE),
                library = library,
            ),
        )
        val album = LibraryAlbum(
            title = "A Complete Album",
            artist = "The Artist",
            representativeUri = "content://albums/cover",
            trackCount = 3,
            year = 2026,
            totalDurationMs = 360_000,
        )

        val ids = session.albumTrackIds(album)

        assertEquals("A Complete Album" to "The Artist", library.request)
        assertEquals(listOf(41L, 7L, 99L), ids)
    }

    @Test
    fun playbackPortForwardsQueueIdsUnchanged() {
        val service = RecordingPlumbingService()
        val delivered = CountDownLatch(2)
        var nextOutcome: Result<UInt>? = null
        var lastOutcome: Result<UInt>? = null
        val controls = controlsFor(service)

        try {
            controls.queueTracksNext(listOf(41L, 7L, 99L)) { result ->
                nextOutcome = result
                delivered.countDown()
            }
            controls.queueTracksLast(listOf(99L, 7L, 41L)) { result ->
                lastOutcome = result
                delivered.countDown()
            }

            assertTrue(delivered.await(5, TimeUnit.SECONDS))
            assertEquals(listOf(41L, 7L, 99L), service.queuedNext)
            assertEquals(listOf(99L, 7L, 41L), service.queuedLast)
            assertEquals(3u, nextOutcome?.getOrThrow())
            assertEquals(3u, lastOutcome?.getOrThrow())
        } finally {
            controls.shutdown()
        }
    }

    @Test
    fun playbackPortKeepsAPartialDeletePartial() {
        val service = RecordingPlumbingService()
        val action = successfulTrashAction()
        val delivered = CountDownLatch(1)
        var outcome: Result<AndroidTrashReport>? = null
        val controls = controlsFor(service, action)

        try {
            controls.deleteTracks(listOf(41L, 7L, 99L)) { result ->
                outcome = result
                delivered.countDown()
            }

            assertTrue(delivered.await(5, TimeUnit.SECONDS))
            assertEquals(listOf(41L, 7L, 99L), service.deleted)
            assertSame(action, service.trashAction)
            assertEquals(
                AndroidTrashReport(
                    removedIds = listOf(41L, 99L),
                    failures = listOf(
                        AndroidTrashFailure(
                            trackId = 7L,
                            uri = "content://tracks/7",
                            error = "permission denied",
                        ),
                    ),
                ),
                outcome?.getOrThrow(),
            )
        } finally {
            controls.shutdown()
        }
    }

    private fun controlsFor(
        service: ReprisePlaybackService,
        trashAction: TrashAction = successfulTrashAction(),
    ) = ActivityPlaybackControls(
        command = { _, operation -> service.operation() },
        connectedService = { service },
        postToMain = { work -> work() },
        setFavouriteAction = { _, _, report -> report(null) },
        trashAction = trashAction,
    )

    private fun successfulTrashAction() = object : TrashAction {
        override fun trash(uri: String): String? = null
    }
}

private class RecordingAlbumLibrary : MusicLibrary(NoHandle) {
    var request: Pair<String, String>? = null

    override fun albumTrackIds(album: String, albumArtist: String): List<Long> {
        request = album to albumArtist
        return listOf(41L, 7L, 99L)
    }
}

private class RecordingPlumbingService : ReprisePlaybackService() {
    var queuedNext: List<Long>? = null
    var queuedLast: List<Long>? = null
    var deleted: List<Long>? = null
    var trashAction: TrashAction? = null

    override fun queueTracksNext(trackIds: List<Long>): UInt {
        queuedNext = trackIds
        return trackIds.size.toUInt()
    }

    override fun queueTracksLast(trackIds: List<Long>): UInt {
        queuedLast = trackIds
        return trackIds.size.toUInt()
    }

    override fun trashTracks(trackIds: List<Long>, action: TrashAction): AndroidTrashReport {
        deleted = trackIds
        trashAction = action
        return AndroidTrashReport(
            removedIds = listOf(41L, 99L),
            failures = listOf(
                AndroidTrashFailure(
                    trackId = 7L,
                    uri = "content://tracks/7",
                    error = "permission denied",
                ),
            ),
        )
    }
}
