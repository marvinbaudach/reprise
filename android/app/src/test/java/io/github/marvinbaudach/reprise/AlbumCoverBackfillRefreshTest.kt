package io.github.marvinbaudach.reprise

import android.graphics.Bitmap
import android.graphics.Color
import androidx.activity.ComponentActivity
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import java.util.concurrent.atomic.AtomicInteger
import kotlin.coroutines.CoroutineContext
import kotlinx.coroutines.CoroutineDispatcher
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import uniffi.reprise_android_ffi.AndroidArtworkSize

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class AlbumCoverBackfillRefreshTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    fun net_7a_the_cover_pass_repaints_a_generated_row() {
        val worker = BackfillManualDispatcher()
        val cover = bitmap(Color.BLUE)
        var coverAvailable = false
        val reads = AtomicInteger()
        val session = LibrarySession(fakeLibrarySessionPort(artworkFor = { _, _ ->
            reads.incrementAndGet()
            if (coverAvailable) COVER_PATH else null
        }))
        val artwork = TrackArtwork(
            resolve = session::artworkFor,
            forgetAlbumArtworkMisses = session::forgetArtworkMisses,
            decode = { path -> if (path == COVER_PATH) cover else null },
            fallback = { _, _, _ -> bitmap(Color.MAGENTA) },
            cache = ArtworkCache(),
            dispatcher = worker,
            onMainThread = { work -> work() },
        )
        val surface = MobileSurfaceViewModel(nowMillis = { 0L })
        surface.bindAlbumCoverRefresh(artwork::albumCoversChanged)
        surface.acceptArtistPhotoProgress(progress(coversDone = 0, coversTotal = 1))
        var row: ArtworkVisual? = null

        try {
            compose.setContent {
                CompositionLocalProvider(LocalTrackArtwork provides artwork) {
                    row = rememberTrackArtworkVisual(TRACK_URI, AndroidArtworkSize.LIST)
                }
            }
            worker.runAll()
            compose.waitForIdle()
            assertEquals(true, row?.generated)

            coverAvailable = true
            compose.runOnIdle {
                surface.acceptArtistPhotoProgress(
                    progress(coversDone = 1, coversTotal = 1),
                )
            }
            compose.waitForIdle()
            worker.runAll()
            compose.waitForIdle()

            assertSame(cover, row?.image?.asAndroidBitmap())
            assertEquals(2, reads.get())
        } finally {
            artwork.shutdown()
        }
    }

    @Test
    fun cover_progress_is_coalesced_to_one_bump_per_two_second_window() {
        val scheduler = FakeCoverScheduler()
        var bumps = 0
        val surface = MobileSurfaceViewModel(
            nowMillis = scheduler::now,
            scheduleAfter = scheduler::scheduleAfter,
        )
        surface.bindAlbumCoverRefresh { bumps += 1 }

        surface.acceptArtistPhotoProgress(progress(coversDone = 1, coversTotal = 5))
        scheduler.time = 1_000L
        surface.acceptArtistPhotoProgress(progress(coversDone = 2, coversTotal = 5))
        assertEquals(0, bumps)

        scheduler.time = 2_000L
        surface.acceptArtistPhotoProgress(progress(coversDone = 3, coversTotal = 5))
        scheduler.runDue()

        assertEquals(1, bumps)
    }

    @Test
    fun completing_the_cover_pass_always_delivers_its_final_bump() {
        val scheduler = FakeCoverScheduler()
        var bumps = 0
        val surface = MobileSurfaceViewModel(
            nowMillis = scheduler::now,
            scheduleAfter = scheduler::scheduleAfter,
        )
        surface.bindAlbumCoverRefresh { bumps += 1 }
        surface.acceptArtistPhotoProgress(progress(coversDone = 1, coversTotal = 4))
        scheduler.time = 2_000L
        surface.acceptArtistPhotoProgress(progress(coversDone = 2, coversTotal = 4))
        scheduler.runDue()
        assertEquals(1, bumps)

        scheduler.time = 2_100L
        surface.acceptArtistPhotoProgress(
            progress(
                phase = ArtistPhotoProgressPhase.COMPLETE,
                coversDone = 3,
                coversTotal = 4,
            ),
        )

        assertEquals(2, bumps)
    }

    @Test
    fun a_new_cover_run_resets_the_coalescing_window() {
        val scheduler = FakeCoverScheduler()
        var bumps = 0
        val surface = MobileSurfaceViewModel(
            nowMillis = scheduler::now,
            scheduleAfter = scheduler::scheduleAfter,
        )
        surface.bindAlbumCoverRefresh { bumps += 1 }
        surface.acceptArtistPhotoProgress(progress(runId = 1, coversDone = 1, coversTotal = 5))

        scheduler.time = 1_900L
        surface.acceptArtistPhotoProgress(progress(runId = 2, coversDone = 1, coversTotal = 5))
        scheduler.time = 2_100L
        surface.acceptArtistPhotoProgress(progress(runId = 2, coversDone = 2, coversTotal = 5))
        scheduler.runDue()
        assertEquals(0, bumps)

        scheduler.time = 3_900L
        surface.acceptArtistPhotoProgress(progress(runId = 2, coversDone = 3, coversTotal = 5))
        scheduler.runDue()

        assertEquals(1, bumps)
    }

    @Test
    fun portrait_only_progress_does_not_bump_album_artwork() {
        val artwork = TrackArtwork(resolve = { _, _ -> null }, cache = ArtworkCache())
        val surface = MobileSurfaceViewModel(nowMillis = { 5_000L })
        surface.bindAlbumCoverRefresh(artwork::albumCoversChanged)

        try {
            surface.acceptArtistPhotoProgress(progress(done = 1))

            assertEquals(0L, artwork.albumCoverRevision)
        } finally {
            artwork.shutdown()
        }
    }

    private fun progress(
        runId: Long = 1,
        phase: ArtistPhotoProgressPhase = ArtistPhotoProgressPhase.RUNNING,
        done: Long = 0,
        coversDone: Long = 0,
        coversTotal: Long = 0,
    ) = ArtistPhotoProgress(
        runId = runId,
        phase = phase,
        done = done + coversDone,
        failed = 0,
        total = 10 + coversTotal,
        coversDone = coversDone,
        coversTotal = coversTotal,
    )

    private fun bitmap(colour: Int): Bitmap =
        Bitmap.createBitmap(8, 8, Bitmap.Config.ARGB_8888).apply { eraseColor(colour) }

    private companion object {
        const val TRACK_URI = "content://tracks/backfilled"
        const val COVER_PATH = "/covers/backfilled.jpg"
    }
}

private class BackfillManualDispatcher : CoroutineDispatcher() {
    private val work = ArrayDeque<Runnable>()

    override fun dispatch(context: CoroutineContext, block: Runnable) {
        work.addLast(block)
    }

    fun runAll() {
        while (work.isNotEmpty()) work.removeFirst().run()
    }
}

private class FakeCoverScheduler {
    var time = 0L
    private val scheduled = ArrayDeque<Pair<Long, () -> Unit>>()

    fun now(): Long = time

    fun scheduleAfter(delayMs: Long, work: () -> Unit) {
        scheduled.addLast(time + delayMs to work)
    }

    fun runDue() {
        val due = scheduled.filter { (dueAt, _) -> dueAt <= time }
        scheduled.removeAll(due.toSet())
        due.forEach { (_, work) -> work() }
    }
}
