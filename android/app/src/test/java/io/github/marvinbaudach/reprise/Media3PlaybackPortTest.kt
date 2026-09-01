package io.github.marvinbaudach.reprise

import android.os.Looper
import android.util.Log
import androidx.media3.common.C
import androidx.media3.common.PlaybackException
import androidx.media3.common.Player
import java.io.FileNotFoundException
import java.lang.reflect.Proxy
import java.util.concurrent.TimeUnit
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Shadows.shadowOf
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.shadows.ShadowLog
import uniffi.reprise_android_ffi.AndroidPlaybackState
import uniffi.reprise_android_ffi.AndroidPlayerEvent
import uniffi.reprise_android_ffi.NoHandle
import uniffi.reprise_android_ffi.PlaybackEventBridge

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class Media3PlaybackPortTest {
    @Test
    fun playbackFaultEmitsItsCauseSummaryAndLogsTheThrowableOnce() {
        val fake = CallbackPlayer(playbackState = Player.STATE_IDLE, playWhenReady = false)
        val events = mutableListOf<AndroidPlayerEvent.Error>()
        val port = Media3PlaybackPort(fake.player) {}
        port.setEventBridge(object : PlaybackEventBridge(NoHandle) {
            override fun emit(generation: ULong, event: AndroidPlayerEvent) {
                if (event is AndroidPlayerEvent.Error) events += event
            }
        })
        val cause = FileNotFoundException("No such file or directory")
        val error = PlaybackException(
            "Source error",
            cause,
            PlaybackException.ERROR_CODE_IO_UNSPECIFIED,
        )
        ShadowLog.clear()

        fake.listener.onPlayerError(error)

        val summary =
            "ERROR_CODE_IO_UNSPECIFIED: Source error — " +
                "FileNotFoundException: No such file or directory"
        assertEquals(listOf(summary), events.map { it.message })
        val logItems = ShadowLog.getLogsForTag("ReprisePlayback")
        assertEquals(1, logItems.size)
        assertEquals(Log.ERROR, logItems.single().type)
        assertEquals(summary, logItems.single().msg)
        assertSame(error, logItems.single().throwable)
        port.release()
    }

    @Test
    fun aMomentaryNotPlayingDoesNotStopPositionEventsForGood() {
        val fake = CallbackPlayer(
            playbackState = Player.STATE_READY,
            playWhenReady = true,
            isPlaying = true,
            currentPosition = 1_000,
        )
        val positions = mutableListOf<Long>()
        val port = Media3PlaybackPort(fake.player) {}
        port.setEventBridge(object : PlaybackEventBridge(NoHandle) {
            override fun emit(generation: ULong, event: AndroidPlayerEvent) {
                if (event is AndroidPlayerEvent.Position) positions += event.positionMs
            }
        })
        fake.listener.onIsPlayingChanged(true)
        shadowOf(Looper.getMainLooper()).idle()

        fake.isPlaying = false
        shadowOf(Looper.getMainLooper()).idleFor(TEST_POSITION_INTERVAL_MS, TimeUnit.MILLISECONDS)
        fake.isPlaying = true
        fake.currentPosition = 2_000
        shadowOf(Looper.getMainLooper()).idleFor(
            TEST_POSITION_INTERVAL_MS * 3,
            TimeUnit.MILLISECONDS,
        )

        assertTrue(
            "position events must resume without another listener callback: $positions",
            positions.size > 1,
        )
        assertEquals(2_000L, positions.last())
        port.release()
    }

    @Test
    fun aPausedPlayerProducesNoPositionEvents() {
        val fake = CallbackPlayer(
            playbackState = Player.STATE_READY,
            playWhenReady = true,
            isPlaying = true,
            currentPosition = 1_000,
        )
        val positions = mutableListOf<Long>()
        val port = Media3PlaybackPort(fake.player) {}
        port.setEventBridge(object : PlaybackEventBridge(NoHandle) {
            override fun emit(generation: ULong, event: AndroidPlayerEvent) {
                if (event is AndroidPlayerEvent.Position) positions += event.positionMs
            }
        })
        fake.listener.onIsPlayingChanged(true)
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(listOf(1_000L), positions)

        fake.isPlaying = false
        fake.playWhenReady = false
        fake.listener.onIsPlayingChanged(false)
        fake.currentPosition = 2_000
        shadowOf(Looper.getMainLooper()).idleFor(
            TEST_POSITION_INTERVAL_MS * 3,
            TimeUnit.MILLISECONDS,
        )

        assertEquals(listOf(1_000L), positions)
        port.release()
    }

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

    @Test
    fun togglePauseReturnsTheRequestedIntentOutcomeRatherThanTransientBuffering() {
        val fake = CallbackPlayer(
            playbackState = Player.STATE_BUFFERING,
            playWhenReady = true,
        )
        val port = Media3PlaybackPort(fake.player) {}

        assertEquals(AndroidPlaybackState.PAUSED, port.togglePause())
        assertEquals(AndroidPlaybackState.PLAYING, port.togglePause())

        port.release()
    }
}

private class CallbackPlayer(
    var playbackState: Int,
    var playWhenReady: Boolean,
    var isPlaying: Boolean = false,
    var currentPosition: Long = 0,
    var duration: Long = 180_000,
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
            "isPlaying" -> isPlaying
            "getPlayWhenReady" -> playWhenReady
            "getPlaybackState" -> playbackState
            "getCurrentPosition" -> currentPosition
            "getDuration" -> duration
            "pause" -> playWhenReady = false
            "play" -> playWhenReady = true
            else -> callbackPlayerDefault(method.returnType)
        }
    } as Player
}

private const val TEST_POSITION_INTERVAL_MS = 500L

private fun callbackPlayerDefault(type: Class<*>): Any? = when (type) {
    Boolean::class.javaPrimitiveType -> false
    Int::class.javaPrimitiveType -> 0
    Long::class.javaPrimitiveType -> 0L
    Float::class.javaPrimitiveType -> 0f
    Double::class.javaPrimitiveType -> 0.0
    else -> null
}
