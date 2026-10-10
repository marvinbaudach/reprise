package io.github.marvinbaudach.reprise

import android.content.Context
import androidx.annotation.OptIn
import androidx.media3.common.Player
import androidx.media3.common.util.UnstableApi
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.exoplayer.audio.TeeAudioProcessor
import androidx.media3.test.utils.FakeClock
import androidx.media3.test.utils.robolectric.TestPlayerRunHelper
import androidx.test.core.app.ApplicationProvider
import java.io.File
import java.nio.ByteBuffer
import java.nio.ByteOrder
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.reprise_android_ffi.AndroidPlaybackSegment

private const val FIRST_VALUE = 1_000
private const val SECOND_VALUE = 2_000
private const val HALF_DB = -6.020599913
private const val DOUBLE_DB = 6.020599913
private const val FIRST_AFTER_HALF = FIRST_VALUE / 2
private const val SECOND_AFTER_DOUBLE = SECOND_VALUE * 2
private const val SECOND_STRETCH_MS = 3_000

/** What the output stage receives, one channel-0 sample per frame. */
private class OutputCapture : TeeAudioProcessor.AudioBufferSink {
    val frames = ArrayList<Int>()
    private var bytesPerFrame = 0

    override fun flush(sampleRateHz: Int, channelCount: Int, encoding: Int) {
        bytesPerFrame = channelCount * Short.SIZE_BYTES
    }

    override fun handleBuffer(buffer: ByteBuffer) {
        val view = buffer.duplicate().order(ByteOrder.LITTLE_ENDIAN)
        while (view.remaining() >= bytesPerFrame) {
            frames += view.getShort(view.position()).toInt()
            view.position(view.position() + bytesPerFrame)
        }
    }
}

/**
 * The real Media3 player, the production sink chain and the production port, over a
 * WAV file whose first stretch is [FIRST_VALUE] and whose second is [SECOND_VALUE].
 * The first CUE track plays the first stretch at -6 dB, the second the rest at +6 dB,
 * so a sample's value says which track it belongs to and its output value says which
 * gain it got.
 */
@OptIn(UnstableApi::class)
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class TrackGainStreamBoundaryTest {
    private val context = ApplicationProvider.getApplicationContext<Context>()

    private fun playTwoCueTracks(cutMs: Int): List<Int> {
        val file = TestWavFile.writeTwoStretches(
            File(context.cacheDir, "gain-boundary-$cutMs.wav"),
            FIRST_VALUE, cutMs, SECOND_VALUE, SECOND_STRETCH_MS,
        )
        val capture = OutputCapture()
        val factory = LivePcmRenderersFactory(context, TeeAudioProcessor(capture))
        val player = ExoPlayer.Builder(context, factory).setClock(FakeClock(true)).build()
        val port = Media3PlaybackPort(player, trackGainSink = factory.trackGainSink) {}
        try {
            port.setNext(
                playbackItem(
                    file.absolutePath, DOUBLE_DB, trackId = 2,
                    segment = AndroidPlaybackSegment(startMs = cutMs.toLong(), endMs = null),
                ),
            )
            port.playPath(
                playbackItem(
                    file.absolutePath, HALF_DB, trackId = 1,
                    segment = AndroidPlaybackSegment(startMs = 0, endMs = cutMs.toLong()),
                ),
            )
            TestPlayerRunHelper.runUntilPlaybackState(player, Player.STATE_ENDED)
            return capture.frames
        } finally {
            port.release()
            player.release()
        }
    }

    @Test
    fun mtp_66_a_cue_cut_inside_a_decoded_buffer_still_switches_gain_at_the_cut_sample() {
        // 3037 ms is no multiple of the extractor's 100 ms WAV chunk.
        val cutMs = 3_037
        val frames = playTwoCueTracks(cutMs)

        val wrong = frames.withIndex().filter { (_, value) ->
            value != FIRST_AFTER_HALF && value != SECOND_AFTER_DOUBLE
        }
        println("frames=${frames.size} firstWrong=${wrong.firstOrNull()} wrong=${wrong.size}")
        val cutFrame = cutMs * TestWavFile.FRAMES_PER_MS
        assertEquals("every sample plays at its own track's gain", emptyList<Any>(), wrong.take(5))
        assertEquals(FIRST_AFTER_HALF, frames[cutFrame - 1])
        assertEquals(SECOND_AFTER_DOUBLE, frames[cutFrame])
    }
}
