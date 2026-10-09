package io.github.marvinbaudach.reprise

import android.content.Context
import android.net.Uri
import android.os.Looper
import androidx.annotation.OptIn
import androidx.media3.common.C
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import androidx.media3.common.Player
import androidx.media3.common.Timeline
import androidx.media3.common.util.UnstableApi
import androidx.media3.datasource.DefaultDataSource
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.exoplayer.RenderersFactory
import androidx.media3.exoplayer.source.DefaultMediaSourceFactory
import androidx.media3.exoplayer.source.MediaSource
import androidx.media3.test.utils.FakeClock
import androidx.media3.test.utils.FakeRenderer
import androidx.media3.test.utils.robolectric.TestPlayerRunHelper
import androidx.test.core.app.ApplicationProvider
import io.github.marvinbaudach.reprise.library.PlaybackKey
import io.github.marvinbaudach.reprise.library.PlaybackRequest
import io.github.marvinbaudach.reprise.library.OpenEndedMediaSourceFactory
import java.io.File
import java.util.Base64
import java.util.concurrent.TimeUnit
import java.util.concurrent.TimeoutException
import java.util.concurrent.atomic.AtomicLong
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import uniffi.reprise_android_ffi.AndroidPlaybackSegment

private const val ONE_SECOND_US = 1_000_000L
private const val HALF_SECOND_MS = 500L
private const val SOURCE_PREPARATION_DEADLINE_MS = 5_000L
private const val PLAYER_RUNNER_DEADLINE_MS = 60_000L

/**
 * A valid two-second mono 8 kHz FLAC whose STREAMINFO total-sample count was
 * patched from 16,000 to 8,000. Metadata padding keeps the file-length estimate
 * above the true sample count without changing the audio or its seek table.
 */
private const val UNDERSTATED_FLAC =
    "ZkxhQwAAACIDIAMgAAANAAANAfQA8AAAH0AiBUsPTuOrp+BUkP0J1XmdAwAASAAAAAAAAAAAAAAAAAAAAAADIAAAAAAAAA+gAAAA" +
        "AAAAAEEDIAAAAAAAAB9AAAAAAAAAAIIDIAAAAAAAAC7gAAAAAAAAAMMDIIQAACggAAAAcmVmZXJlbmNlIGxpYkZMQUMgMS41LjAg" +
        "MjAyNTAyMTEAAAAA//h0CAADH/cAAABK+P/4dAgBAx+cAAAA1/n/+HQIAgMfIQAAAPD///h0CAMDH0oAAABt/v/4dAgEAx9cAAAA" +
        "0ov/+HQIBQMfNwAAAE+K//h0CAYDH4oAAABojP/4dAgHAx/hAAAA9Y3/+HQICAMfpgAAAJZj//h0CAkDH80AAAALYv/4dAgKAx9w" +
        "AAAALGT/+HQICwMfGwAAALFl//h0CAwDHw0AAAAOEP/4dAgNAx9mAAAAkxH/+HQIDgMf2wAAALQX//h0CA8DH7AAAAApFv/4dAgQ" +
        "Ax9VAAAAc8v/+HQIEQMfPgAAAO7K//h0CBIDH4MAAADJzP/4dAgTAx/oAAAAVM0="

@OptIn(UnstableApi::class)
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class Mtp68OpenEndedMediaSourceTest {
    private val context = ApplicationProvider.getApplicationContext<Context>()
    private val file = File(context.cacheDir, "mtp-68-understated.flac").apply {
        writeBytes(
            TestFlacFile.withMetadataPadding(
                Base64.getDecoder().decode(UNDERSTATED_FLAC),
                TWO_SECOND_PADDING_BYTES,
            ),
        )
    }

    @Test
    fun mtp_68_generated_no_seektable_flac_prepares_on_the_default_factory() {
        val generated = TestFlacFile.writeTenMinuteUnderstated(
            File(context.cacheDir, "mtp-68-no-seektable-validation.flac"),
        )
        val counts = CountingDataSourceFactory(DefaultDataSource.Factory(context))

        val result = prepareToReady(
            DefaultMediaSourceFactory(counts),
            cueItem(generated, startMs = 30_000L, endMs = 31_000L),
            counts,
        )

        assertTrue("expected the generated fixture to reach READY: $result", result.ready)
        assertTrue("expected the generated fixture to be read", result.counts.bytesRead > 0)
    }

    @Test
    fun mtp_68_an_open_ended_clip_is_not_cut_at_the_header_duration() {
        // 8,000 bytes / (0.25 * 1 channel * 16 bits / 8) = 16,000 samples.
        assertEquals(8_000L, file.length())
        val item = cueItem(startMs = HALF_SECOND_MS, endMs = null)

        val headerDurationUs = preparedDurationUs(DefaultMediaSourceFactory(context), item)
        val observedDurations = mutableListOf<Long>()
        val openEndedDurationUs = preparedKnownDurationUs(
            OpenEndedMediaSourceFactory(context),
            listOf(item),
            observedDurations,
        )

        assertEquals(ONE_SECOND_US - HALF_SECOND_MS * 1_000L, headerDurationUs)
        assertTrue(
            "expected about 1.5 s after the clip start, got $openEndedDurationUs us",
            openEndedDurationUs in 1_400_000L..1_600_000L,
        )
        assertTrue("observed durations: $observedDurations", observedDurations.contains(C.TIME_UNSET))
        assertNotEquals(500_000L, openEndedDurationUs)
        assertTrue("observed header-derived duration: $observedDurations", 500_000L !in observedDurations)
    }

    @Test
    fun mtp_68_a_highly_compressible_understated_flac_plays_to_its_true_end() {
        // Constant subframes compress about 800:1, far past any file-length-based guess: the
        // 13 KB file would estimate to about 26,000 samples, so only the audio itself can tell
        // the open-ended clip that it runs on to ten minutes instead of the declared minute.
        val generated = TestFlacFile.writeTenMinuteCompressibleUnderstated(
            File(context.cacheDir, "mtp-68-compressible-understated.flac"),
        )
        val startMs = 30_000L

        val learnedDurationUs = preparedKnownDurationUs(
            OpenEndedMediaSourceFactory(context),
            listOf(cueItem(generated, startMs, endMs = null)),
        )

        val trueDurationUs = TestFlacFile.TRUE_SAMPLES * ONE_SECOND_US / TestFlacFile.SAMPLE_RATE
        val expectedUs = trueDurationUs - startMs * 1_000L
        assertTrue(
            "expected about ${expectedUs / ONE_SECOND_US} s after the clip start, " +
                "got ${learnedDurationUs / ONE_SECOND_US} s",
            learnedDurationUs in (expectedUs - ONE_SECOND_US)..(expectedUs + ONE_SECOND_US),
        )
    }

    @Test
    fun mtp_68_an_open_ended_clip_lets_the_queue_move_on_to_the_next_item() {
        val first = cueItem(startMs = HALF_SECOND_MS, endMs = null)
        val second = wholeFileItem()

        val firstDurationUs = preparedKnownDurationUs(
            OpenEndedMediaSourceFactory(context),
            listOf(first, second),
        )

        assertTrue(
            "the first queue window must become known after loading, got $firstDurationUs",
            firstDurationUs != C.TIME_UNSET,
        )
    }

    @Test
    fun mtp_68_bounded_segments_and_whole_files_keep_their_timeline_durations() {
        val factory = OpenEndedMediaSourceFactory(context)

        val boundedDurationUs = preparedDurationUs(
            factory,
            cueItem(startMs = 250L, endMs = 750L),
        )
        val wholeFileDurationUs = preparedDurationUs(factory, wholeFileItem())

        assertEquals(500_000L, boundedDurationUs)
        assertEquals(ONE_SECOND_US, wholeFileDurationUs)
    }

    @Test
    fun mtp_68_an_open_ended_source_keeps_the_item_updatable_in_place() {
        val before = cueItem(startMs = HALF_SECOND_MS, endMs = null)
            .buildUpon()
            .setMediaMetadata(MediaMetadata.Builder().setTitle("Track 3").build())
            .build()
        val after = before.buildUpon()
            .setMediaMetadata(before.mediaMetadata.buildUpon().setArtist("New Order").build())
            .build()

        val source = OpenEndedMediaSourceFactory(context).createMediaSource(before)

        assertTrue(source.canUpdateMediaItem(after))
    }

    private fun cueItem(startMs: Long, endMs: Long?): MediaItem {
        return cueItem(file, startMs, endMs)
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

    private fun wholeFileItem(): MediaItem = MediaItem.Builder()
        .setUri(Uri.fromFile(file))
        .setTag(PlaybackRequest(PlaybackKey(68, file.toURI().toString()), segment = null))
        .build()

    private fun preparedDurationUs(factory: MediaSource.Factory, item: MediaItem): Long {
        val player = ExoPlayer.Builder(context)
            .setMediaSourceFactory(factory)
            .build()
        try {
            player.setMediaItem(item)
            player.prepare()
            val deadline = System.nanoTime() +
                TimeUnit.MILLISECONDS.toNanos(SOURCE_PREPARATION_DEADLINE_MS)
            while (
                !hasExtractedTimeline(player) &&
                player.playerError == null &&
                System.nanoTime() < deadline
            ) {
                shadowOf(Looper.getMainLooper()).idle()
                Thread.yield()
            }
            check(player.playerError == null) { "source failed: ${player.playerError}" }
            check(hasExtractedTimeline(player)) {
                "source did not publish its extracted timeline; state=${player.playbackState}"
            }
            return player.currentTimeline.getWindow(0, Timeline.Window()).durationUs
        } finally {
            player.release()
        }
    }

    private fun preparedKnownDurationUs(
        factory: MediaSource.Factory,
        items: List<MediaItem>,
        observedDurations: MutableList<Long> = mutableListOf(),
    ): Long {
        // Robolectric has no audio decoder, and a player with no enabled renderer ends as soon as
        // its source has no known duration, which can be before the load that would reveal it.
        // A renderer that consumes the samples keeps playback running until the end of the audio.
        val player = ExoPlayer.Builder(
            context,
            RenderersFactory { _, _, _, _, _ -> arrayOf(FakeRenderer(C.TRACK_TYPE_AUDIO)) },
        )
            .setMediaSourceFactory(factory)
            .setClock(FakeClock(true))
            .build()
        try {
            val knownDurationUs = AtomicLong(C.TIME_UNSET)
            player.addListener(
                object : Player.Listener {
                    override fun onTimelineChanged(timeline: Timeline, reason: Int) {
                        if (timeline.isEmpty) return
                        val window = timeline.getWindow(0, Timeline.Window())
                        if (observedDurations.lastOrNull() != window.durationUs) {
                            observedDurations += window.durationUs
                        }
                        if (!window.isPlaceholder && window.durationUs != C.TIME_UNSET) {
                            knownDurationUs.set(window.durationUs)
                        }
                    }
                },
            )
            player.setMediaItems(items)
            player.prepare()
            TestPlayerRunHelper.play(player)
                .withTimeoutMs(PLAYER_RUNNER_DEADLINE_MS)
                .untilBackgroundThreadCondition { knownDurationUs.get() != C.TIME_UNSET }
            check(player.playerError == null) { "source failed: ${player.playerError}" }
            return knownDurationUs.get()
        } finally {
            player.release()
        }
    }

    private fun hasExtractedTimeline(player: Player): Boolean {
        val timeline = player.currentTimeline
        return !timeline.isEmpty && !timeline.getWindow(0, Timeline.Window()).isPlaceholder
    }

    private fun prepareToReady(
        factory: MediaSource.Factory,
        item: MediaItem,
        counts: CountingDataSourceFactory,
    ): PreparationMeasurement {
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
                    .withTimeoutMs(MEASUREMENT_DEADLINE_MS)
                    .untilState(Player.STATE_READY)
            } catch (_: TimeoutException) {
                ready = false
            }
            check(player.playerError == null) { "source failed: ${player.playerError}" }
            return PreparationMeasurement(
                counts = counts.counts(),
                wallTimeMs = TimeUnit.NANOSECONDS.toMillis(System.nanoTime() - started),
                ready = ready,
            )
        } finally {
            player.release()
        }
    }

}

private data class PreparationMeasurement(
    val counts: DataSourceCounts,
    val wallTimeMs: Long,
    val ready: Boolean,
)

private const val MEASUREMENT_DEADLINE_MS = 60_000L
// 422 fixture bytes + four-byte metadata header + padding = 8,000 bytes.
private const val TWO_SECOND_PADDING_BYTES = 7_574
