package io.github.marvinbaudach.reprise

import androidx.media3.exoplayer.audio.TeeAudioProcessor
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertThrows
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class LivePcmRenderersFactoryTest {
    private val context = RuntimeEnvironment.getApplication()
    private val processor = TeeAudioProcessor(LivePcmBufferSink())

    @Test
    fun pcm16OutputBuildsAnAudioSink() {
        val sink = LivePcmRenderersFactory(context, processor).buildAudioSink(
            context = context,
            enableFloatOutput = false,
            enableAudioOutputPlaybackParameters = true,
        )

        assertNotNull(sink)
    }

    @Test
    fun floatOutputIsExplicitlyRejectedBecauseTheTapRequiresPcm16() {
        val factory = LivePcmRenderersFactory(context, processor)

        assertThrows(IllegalArgumentException::class.java) {
            factory.buildAudioSink(
                context = context,
                enableFloatOutput = true,
                enableAudioOutputPlaybackParameters = true,
            )
        }
    }
}
