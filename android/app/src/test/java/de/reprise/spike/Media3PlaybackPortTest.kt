package de.reprise.spike

import android.os.Looper
import androidx.media3.common.C
import androidx.media3.common.Player
import java.lang.reflect.Proxy
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.reprise_android_ffi.AndroidPlaybackState
import uniffi.reprise_android_ffi.AndroidPlayerEvent
import uniffi.reprise_android_ffi.NoHandle
import uniffi.reprise_android_ffi.PlaybackEventBridge

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class Media3PlaybackPortTest {
    @Test
    fun playIntentChangesWhileBufferingPublishBothPauseAndResumeStates() {
        val fake = CallbackPlayer(
            playbackState = Player.STATE_BUFFERING,
            playWhenReady = false,
        )
        val states = mutableListOf<AndroidPlaybackState>()
        val port = Media3PlaybackPort(fake.player) {}
        port.setEventBridge(object : PlaybackEventBridge(NoHandle) {
            override fun emit(generation: ULong, event: AndroidPlayerEvent) {
                if (event is AndroidPlayerEvent.StateChanged) states += event.state
            }
        })

        fake.listener.onPlaybackStateChanged(Player.STATE_BUFFERING)
        fake.playWhenReady = true
        fake.listener.onPlayWhenReadyChanged(
            true,
            Player.PLAY_WHEN_READY_CHANGE_REASON_USER_REQUEST,
        )
        fake.playWhenReady = false
        fake.listener.onPlayWhenReadyChanged(
            false,
            Player.PLAY_WHEN_READY_CHANGE_REASON_USER_REQUEST,
        )

        assertEquals(
            listOf(
                AndroidPlaybackState.PAUSED,
                AndroidPlaybackState.BUFFERING,
                AndroidPlaybackState.PAUSED,
            ),
            states,
        )
        port.release()
    }
}

private class CallbackPlayer(
    var playbackState: Int,
    var playWhenReady: Boolean,
) {
    lateinit var listener: Player.Listener

    val player: Player = Proxy.newProxyInstance(
        Player::class.java.classLoader,
        arrayOf(Player::class.java),
    ) { _, method, arguments ->
        when (method.name) {
            "addListener" -> listener = arguments?.single() as Player.Listener
            "getApplicationLooper" -> Looper.getMainLooper()
            "getAudioSessionId" -> C.AUDIO_SESSION_ID_UNSET
            "getIsPlaying" -> false
            "getPlayWhenReady" -> playWhenReady
            "getPlaybackState" -> playbackState
            else -> callbackPlayerDefault(method.returnType)
        }
    } as Player
}

private fun callbackPlayerDefault(type: Class<*>): Any? = when (type) {
    Boolean::class.javaPrimitiveType -> false
    Int::class.javaPrimitiveType -> 0
    Long::class.javaPrimitiveType -> 0L
    Float::class.javaPrimitiveType -> 0f
    Double::class.javaPrimitiveType -> 0.0
    else -> null
}
