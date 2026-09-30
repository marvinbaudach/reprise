package io.github.marvinbaudach.reprise

import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.reprise_android_ffi.AndroidTrashFailure
import uniffi.reprise_android_ffi.AndroidTrashReport
import uniffi.reprise_android_ffi.TrashAction

/**
 * A deletion tells the activity the library changed, and only when something
 * really left it.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class LibraryChangedHookTest {
    private val failure = AndroidTrashFailure(trackId = 7L, uri = "content://tracks/7", error = "denied")

    @Test
    fun aDeletionThatRemovedRowsTellsTheActivity() {
        val log = deleteAndWait(AndroidTrashReport(removedIds = listOf(41L), failures = emptyList()))

        assertEquals(listOf("changed", "reported"), log)
    }

    @Test
    fun aPartialDeletionStillTellsTheActivityBecauseSomeRowsAreGone() {
        val log = deleteAndWait(AndroidTrashReport(removedIds = listOf(41L), failures = listOf(failure)))

        assertEquals(listOf("changed", "reported"), log)
    }

    @Test
    fun aDeletionThatRemovedNothingLeavesTheLibraryAlone() {
        val log = deleteAndWait(AndroidTrashReport(removedIds = emptyList(), failures = listOf(failure)))

        assertEquals(listOf("reported"), log)
    }

    @Test
    fun aDeletionThatFailedOutrightLeavesTheLibraryAlone() {
        val log = deleteAndWait(report = null)

        assertEquals(listOf("reported"), log)
    }

    private fun deleteAndWait(report: AndroidTrashReport?): List<String> {
        val log = java.util.Collections.synchronizedList(mutableListOf<String>())
        val service = object : ReprisePlaybackService() {
            override fun trashTracks(trackIds: List<Long>, action: TrashAction): AndroidTrashReport =
                report ?: error("the provider refused")
        }
        val controls = ActivityPlaybackControls(
            command = { _, operation -> service.operation() },
            connectedService = { service },
            postToMain = { work -> work() },
            setFavouriteAction = { _, _, done -> done(null) },
            trashAction = object : TrashAction {
                override fun trash(uri: String): String? = null
            },
            playTrackIdsAction = { _, _ -> },
            onLibraryChanged = { log += "changed" },
        )
        val delivered = CountDownLatch(1)
        try {
            controls.deleteTracks(listOf(41L, 7L)) {
                log += "reported"
                delivered.countDown()
            }
            assertTrue(delivered.await(5, TimeUnit.SECONDS))
        } finally {
            controls.shutdown()
        }
        return log.toList()
    }
}
