package io.github.marvinbaudach.reprise

import android.content.Context
import android.media.AudioManager
import androidx.media3.common.DeviceInfo
import androidx.media3.common.Player
import java.lang.reflect.Proxy
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements
import org.robolectric.shadows.ShadowAudioManager

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36], shadows = [RecordingAudioManagerShadow::class])
class CoreControlledPlayerTest {
    private val context = RuntimeEnvironment.getApplication()
    private val audio = context.getSystemService(Context.AUDIO_SERVICE) as AudioManager

    @Test
    fun mediaSessionTransportReturnsToCore() {
        var playWhenReady = false
        val calls = mutableListOf<String>()
        val commands = RecordingCommands(audio, transportEvents = calls)
        val controlled = controlledPlayer(
            player = player(playWhenReady = { playWhenReady }),
            commands = commands,
        )

        controlled.play()
        playWhenReady = true
        controlled.pause()
        controlled.seekToNext()
        controlled.seekToPrevious()
        controlled.seekToPreviousMediaItem()

        assertEquals(listOf("toggle", "toggle", "next", "queue-previous", "queue-previous"), calls)
    }

    @Test
    fun deviceInfoIsRemoteWhileReadyOrBufferingWithPlaybackIntended() {
        var playWhenReady = true
        var playbackState = Player.STATE_READY
        val controlled = controlledPlayer(
            player = player(
                playWhenReady = { playWhenReady },
                playbackState = { playbackState },
            ),
        )

        assertEquals(DeviceInfo.PLAYBACK_TYPE_REMOTE, controlled.deviceInfo.playbackType)
        playbackState = Player.STATE_BUFFERING
        assertEquals(DeviceInfo.PLAYBACK_TYPE_REMOTE, controlled.deviceInfo.playbackType)

        playWhenReady = false
        assertEquals(DeviceInfo.PLAYBACK_TYPE_LOCAL, controlled.deviceInfo.playbackType)
        playWhenReady = true
        playbackState = Player.STATE_IDLE
        assertEquals(DeviceInfo.PLAYBACK_TYPE_LOCAL, controlled.deviceInfo.playbackType)
    }

    @Test
    fun disabledSwitchKeepsDeviceInfoLocal() {
        val controlled = controlledPlayer(
            commands = RecordingCommands(audio, enabled = { false }),
        )

        assertEquals(DeviceInfo.PLAYBACK_TYPE_LOCAL, controlled.deviceInfo.playbackType)
    }

    @Test
    fun playbackAndSettingChangesPublishTheComputedDeviceInfo() {
        var playWhenReady = false
        var playbackState = Player.STATE_READY
        var enabled = true
        val wrappedListeners = mutableListOf<Player.Listener>()
        val controlled = controlledPlayer(
            player = player(
                playWhenReady = { playWhenReady },
                playbackState = { playbackState },
                onAddListener = { wrappedListeners += it },
            ),
            commands = RecordingCommands(audio, enabled = { enabled }),
        )
        val published = mutableListOf<Int>()
        controlled.addListener(object : Player.Listener {
            override fun onDeviceInfoChanged(deviceInfo: DeviceInfo) {
                published += deviceInfo.playbackType
            }
        })

        playWhenReady = true
        wrappedListeners.first().onPlayWhenReadyChanged(true, Player.PLAY_WHEN_READY_CHANGE_REASON_USER_REQUEST)
        playbackState = Player.STATE_BUFFERING
        wrappedListeners.first().onPlaybackStateChanged(Player.STATE_BUFFERING)
        enabled = false
        controlled.refreshDeviceInfo()

        assertEquals(
            listOf(DeviceInfo.PLAYBACK_TYPE_REMOTE, DeviceInfo.PLAYBACK_TYPE_LOCAL),
            published,
        )
    }

    @Test
    @Suppress("DEPRECATION")
    fun deviceVolumeCommandsAreAvailableEvenWhenTheWrappedPlayerRefusesThem() {
        val controlled = controlledPlayer()

        assertTrue(controlled.availableCommands.contains(Player.COMMAND_ADJUST_DEVICE_VOLUME))
        assertTrue(controlled.isCommandAvailable(Player.COMMAND_ADJUST_DEVICE_VOLUME))
    }

    @Test
    fun upDownRockRestoresVolumeSkipsForwardAndTicksOnce() {
        val clock = FakePlayerClock()
        val commands = RecordingCommands(audio)
        val controlled = controlledPlayer(commands = commands, now = clock::now)
        audio.setStreamVolume(AudioManager.STREAM_MUSIC, 10, 0)

        controlled.increaseDeviceVolume(0)
        clock.advance(400)
        controlled.decreaseDeviceVolume(0)

        assertEquals(listOf("next:10", "tick"), commands.events)
        assertEquals(10, audio.getStreamVolume(AudioManager.STREAM_MUSIC))
    }

    @Test
    fun downUpRockRestoresVolumeSkipsBackwardAndTicksOnce() {
        val clock = FakePlayerClock()
        val commands = RecordingCommands(audio)
        val controlled = controlledPlayer(commands = commands, now = clock::now)
        audio.setStreamVolume(AudioManager.STREAM_MUSIC, 10, 0)

        controlled.decreaseDeviceVolume(0)
        clock.advance(400)
        controlled.increaseDeviceVolume(0)

        assertEquals(listOf("previous:10", "tick"), commands.events)
        assertEquals(10, audio.getStreamVolume(AudioManager.STREAM_MUSIC))
    }

    @Test
    fun oneTapStepsImmediatelyAndNeverTicks() {
        val commands = RecordingCommands(audio)
        val controlled = controlledPlayer(commands = commands)
        audio.setStreamVolume(AudioManager.STREAM_MUSIC, 10, 0)
        RecordingAudioManagerShadow.lastAdjustment = null

        controlled.increaseDeviceVolume(0)

        assertEquals(11, audio.getStreamVolume(AudioManager.STREAM_MUSIC))
        assertEquals(
            AudioAdjustment(
                streamType = AudioManager.STREAM_MUSIC,
                direction = AudioManager.ADJUST_RAISE,
                flags = AudioManager.FLAG_SHOW_UI,
            ),
            RecordingAudioManagerShadow.lastAdjustment,
        )
        assertFalse(commands.events.contains("tick"))
    }

    @Test
    fun foregroundRockRemainsTwoVolumeSteps() {
        val clock = FakePlayerClock()
        val commands = RecordingCommands(audio, foreground = { true })
        val controlled = controlledPlayer(commands = commands, now = clock::now)
        audio.setStreamVolume(AudioManager.STREAM_MUSIC, 10, 0)

        controlled.increaseDeviceVolume(0)
        clock.advance(100)
        controlled.decreaseDeviceVolume(0)

        assertEquals(10, audio.getStreamVolume(AudioManager.STREAM_MUSIC))
        assertTrue(commands.events.isEmpty())
    }

    @Test
    fun setDeviceVolumeForwardsToTheMusicStream() {
        val controlled = controlledPlayer()

        controlled.setDeviceVolume(7, 0)

        assertEquals(7, audio.getStreamVolume(AudioManager.STREAM_MUSIC))
    }

    private fun controlledPlayer(
        player: Player = player(),
        commands: CoreControlledPlayer.Commands = RecordingCommands(audio),
        now: () -> Long = { 0L },
    ) = CoreControlledPlayer(player, commands, context, now)

    private fun player(
        playWhenReady: () -> Boolean = { true },
        playbackState: () -> Int = { Player.STATE_READY },
        onAddListener: (Player.Listener) -> Unit = {},
    ): Player = Proxy.newProxyInstance(
        Player::class.java.classLoader,
        arrayOf(Player::class.java),
    ) { _, method, arguments ->
        when (method.name) {
            "getPlayWhenReady" -> playWhenReady()
            "getPlaybackState" -> playbackState()
            "addListener" -> onAddListener(arguments?.first() as Player.Listener)
            "getAvailableCommands" -> Player.Commands.Builder().build()
            "isCommandAvailable" -> false
            else -> primitiveDefault(method.returnType)
        }
    } as Player
}

data class AudioAdjustment(
    val streamType: Int,
    val direction: Int,
    val flags: Int,
)

@Implements(AudioManager::class)
class RecordingAudioManagerShadow : ShadowAudioManager() {
    @Implementation
    override fun adjustStreamVolume(streamType: Int, direction: Int, flags: Int) {
        lastAdjustment = AudioAdjustment(streamType, direction, flags)
        super.adjustStreamVolume(streamType, direction, flags)
    }

    companion object {
        var lastAdjustment: AudioAdjustment? = null
    }
}

private class RecordingCommands(
    private val audio: AudioManager,
    private val enabled: () -> Boolean = { true },
    private val foreground: () -> Boolean = { false },
    private val transportEvents: MutableList<String>? = null,
) : CoreControlledPlayer.Commands {
    val events = mutableListOf<String>()

    override fun togglePause() {
        transportEvents?.add("toggle")
    }

    override fun next() {
        transportEvents?.add("next") ?: events.add(
            "next:${audio.getStreamVolume(AudioManager.STREAM_MUSIC)}",
        )
    }

    override fun previousInQueueOrder() {
        transportEvents?.add("queue-previous") ?: events.add(
            "previous:${audio.getStreamVolume(AudioManager.STREAM_MUSIC)}",
        )
    }

    override fun isActivityInForeground(): Boolean = foreground()

    override fun volumeKeySkipGestureEnabled(): Boolean = enabled()

    override fun hapticTick() {
        events += "tick"
    }
}

private class FakePlayerClock {
    private var timeMs = 0L

    fun now(): Long = timeMs

    fun advance(milliseconds: Long) {
        timeMs += milliseconds
    }
}

private fun primitiveDefault(type: Class<*>): Any? = when (type) {
    Boolean::class.javaPrimitiveType -> false
    Int::class.javaPrimitiveType -> 0
    Long::class.javaPrimitiveType -> 0L
    Float::class.javaPrimitiveType -> 0f
    Double::class.javaPrimitiveType -> 0.0
    else -> null
}
