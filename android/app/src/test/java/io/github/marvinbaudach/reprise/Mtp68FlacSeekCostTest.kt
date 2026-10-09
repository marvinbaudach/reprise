package io.github.marvinbaudach.reprise

import android.content.Context
import android.net.Uri
import androidx.annotation.OptIn
import androidx.media3.common.C
import androidx.media3.common.MediaItem
import androidx.media3.common.Player
import androidx.media3.common.util.UnstableApi
import androidx.media3.datasource.DefaultDataSource
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.exoplayer.source.DefaultMediaSourceFactory
import androidx.media3.exoplayer.source.MediaSource
import androidx.media3.test.utils.FakeClock
import androidx.media3.test.utils.robolectric.TestPlayerRunHelper
import androidx.test.core.app.ApplicationProvider
import io.github.marvinbaudach.reprise.library.OpenEndedMediaSourceFactory
import io.github.marvinbaudach.reprise.library.PlaybackKey
import io.github.marvinbaudach.reprise.library.PlaybackRequest
import java.io.File
import java.util.concurrent.TimeUnit
import java.util.concurrent.TimeoutException
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.reprise_android_ffi.AndroidPlaybackSegment

@OptIn(UnstableApi::class)
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class Mtp68FlacSeekCostTest {
    private val context = ApplicationProvider.getApplicationContext<Context>()

    @Test
    fun mtp_68_open_ended_seek_cost_stays_close_to_the_bounded_baseline() {
        val understated = TestFlacFile.writeTenMinuteUnderstated(
            File(context.cacheDir, "mtp-68-no-seektable-measurement.flac"),
        )
        val accurate = TestFlacFile.writeTenMinuteAccurate(
            File(context.cacheDir, "mtp-68-no-seektable-baseline.flac"),
        )
        val openEndedCounts = CountingDataSourceFactory(DefaultDataSource.Factory(context))

        // Keeping the duration visible isolates Media3's binary seeker. The production
        // factory still reports unknown duration until load completion.
        val openEnded = prepareToReady(
            OpenEndedMediaSourceFactory(
                context,
                openEndedCounts,
                reportUnknownDuration = false,
            ),
            cueItem(understated, startMs = NINE_MINUTES_MS, endMs = null),
            openEndedCounts,
        )
        val baseline = (1..BASELINE_RUNS).map {
            val counts = CountingDataSourceFactory(DefaultDataSource.Factory(context))
            prepareToReady(
                DefaultMediaSourceFactory(counts),
                cueItem(accurate, startMs = NINE_MINUTES_MS, endMs = TEN_MINUTES_MS),
                counts,
            )
        }.maxBy { it.counts.bytesRead }
        println("MTP-68 bounded baseline: $baseline")
        println("MTP-68 estimated open-ended: $openEnded")

        assertTrue("bounded baseline did not reach READY: $baseline", baseline.ready)
        assertTrue("open-ended source did not reach READY: $openEnded", openEnded.ready)
        assertTrue(
            "open-ended opens ${openEnded.counts.opens}, baseline ${baseline.counts.opens}",
            openEnded.counts.opens <= baseline.counts.opens * MEASUREMENT_MULTIPLIER,
        )
        assertTrue(
            "open-ended bytes ${openEnded.counts.bytesRead}, baseline ${baseline.counts.bytesRead}",
            openEnded.counts.bytesRead <= baseline.counts.bytesRead * MEASUREMENT_MULTIPLIER,
        )
    }

    private fun cueItem(file: File, startMs: Long, endMs: Long?): MediaItem {
        val segment = AndroidPlaybackSegment(startMs = startMs, endMs = endMs)
        return MediaItem.Builder()
            .setUri(Uri.fromFile(file))
            .setTag(PlaybackRequest(PlaybackKey(68, file.toURI().toString()), segment))
            .setClippingConfiguration(
                MediaItem.ClippingConfiguration.Builder()
                    .setStartPositionMs(startMs)
                    .setEndPositionMs(endMs ?: C.TIME_END_OF_SOURCE)
                    .build(),
            )
            .build()
    }

    private fun prepareToReady(
        factory: MediaSource.Factory,
        item: MediaItem,
        counts: CountingDataSourceFactory,
    ): SeekPreparationMeasurement {
        val player = ExoPlayer.Builder(context)
            .setMediaSourceFactory(factory)
            .setClock(FakeClock(true))
            .build()
        val started = System.nanoTime()
        try {
            player.setMediaItem(item)
            player.prepare()
            var ready = true
            try {
                TestPlayerRunHelper.play(player)
                    .withTimeoutMs(SEEK_MEASUREMENT_DEADLINE_MS)
                    .untilState(Player.STATE_READY)
            } catch (_: TimeoutException) {
                ready = false
            }
            check(player.playerError == null) { "source failed: ${player.playerError}" }
            return SeekPreparationMeasurement(
                counts = counts.counts(),
                wallTimeMs = TimeUnit.NANOSECONDS.toMillis(System.nanoTime() - started),
                ready = ready,
            )
        } finally {
            player.release()
        }
    }
}

private data class SeekPreparationMeasurement(
    val counts: DataSourceCounts,
    val wallTimeMs: Long,
    val ready: Boolean,
)

private const val NINE_MINUTES_MS = 9L * 60L * 1_000L
private const val TEN_MINUTES_MS = 10L * 60L * 1_000L
private const val MEASUREMENT_MULTIPLIER = 8L
private const val SEEK_MEASUREMENT_DEADLINE_MS = 60_000L
private const val BASELINE_RUNS = 3
