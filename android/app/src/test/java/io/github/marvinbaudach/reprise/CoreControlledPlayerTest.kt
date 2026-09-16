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

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class CoreControlledPlayerTest {
    private val context = RuntimeEnvironment.getApplication()
    private val audio = context.getSystemService(Context.AUDIO_SERVICE) as AudioManager

    @Test
    fun mediaSessionTransportReturnsToCore() {
        var playWhenReady = false
        val calls = mutableListOf<String>()
        val commands = object : CoreControlledPlayer.Commands {
            override fun togglePause() {
                calls += "toggle"
            }

            override fun next() {
                calls += "next"
            }

            override fun previousInQueueOrder() {
                calls += "queue-previous"
            }

            override fun isActivityInForeground(): Boolean = false

            override fun volumeKeyTrackSwitchEnabled(): Boolean = true

            override fun hapticTick() = Unit
        }
        val controlled = controlledPlayer(
            player = player(isPlaying = { false }, playWhenReady = { playWhenReady }),
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
    fun deviceInfoIsRemoteOnlyWhilePlayingAndTheSwitchIsOn() {
        var playing = false
        var enabled = true
        val controlled = controlledPlayer(
            player = player(isPlaying = { playing }),
            commands = RecordingCommands(audio, enabled = { enabled }),
        )

        assertEquals(DeviceInfo.PLAYBACK_TYPE_LOCAL, controlled.deviceInfo.playbackType)
        playing = true
        assertEquals(DeviceInfo.PLAYBACK_TYPE_REMOTE, controlled.deviceInfo.playbackType)
        enabled = false
        assertEquals(DeviceInfo.PLAYBACK_TYPE_LOCAL, controlled.deviceInfo.playbackType)
    }

    @Test
    fun playbackAndSettingChangesPublishTheComputedDeviceInfo() {
        var playing = false
        var enabled = true
        val wrappedListeners = mutableListOf<Player.Listener>()
        val controlled = controlledPlayer(
            player = player(
                isPlaying = { playing },
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

        playing = true
        wrappedListeners.first().onIsPlayingChanged(true)
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
    fun disabledSwitchKeepsEveryRelativeAdjustmentAsAVolumeStep() {
        val clock = FakePlayerClock()
        val commands = RecordingCommands(audio, enabled = { false })
        val controlled = controlledPlayer(commands = commands, now = clock::now)
        audio.setStreamVolume(AudioManager.STREAM_MUSIC, 10, 0)

        controlled.increaseDeviceVolume(0)
        clock.advance(250)
        controlled.increaseDeviceVolume(0)
        clock.advance(50)
        controlled.increaseDeviceVolume(0)

        assertTrue(commands.events.isEmpty())
        assertTrue(audio.getStreamVolume(AudioManager.STREAM_MUSIC) > 10)
    }

    @Test
    fun oneHoldSkipsOnceAndAnAbsoluteSliderSetNeverSkips() {
        val clock = FakePlayerClock()
        val commands = RecordingCommands(audio)
        val controlled = controlledPlayer(commands = commands, now = clock::now)
        audio.setStreamVolume(AudioManager.STREAM_MUSIC, 10, 0)

        controlled.increaseDeviceVolume(0)
        clock.advance(250)
        controlled.increaseDeviceVolume(0)
        clock.advance(50)
        controlled.increaseDeviceVolume(0)
        clock.advance(50)
        controlled.increaseDeviceVolume(0)

        assertEquals(listOf("next:10", "tick"), commands.events)
        assertEquals(10, audio.getStreamVolume(AudioManager.STREAM_MUSIC))

        controlled.setDeviceVolume(7, 0)
        assertEquals(7, audio.getStreamVolume(AudioManager.STREAM_MUSIC))
        assertEquals(listOf("next:10", "tick"), commands.events)
    }

    @Test
    fun aStepAndASwallowNeverTick() {
        val clock = FakePlayerClock()
        val commands = RecordingCommands(audio)
        val controlled = controlledPlayer(commands = commands, now = clock::now)
        audio.setStreamVolume(AudioManager.STREAM_MUSIC, 10, 0)

        controlled.decreaseDeviceVolume()
        assertFalse(commands.events.contains("tick"))
        clock.advance(250)
        controlled.decreaseDeviceVolume()
        clock.advance(50)
        controlled.decreaseDeviceVolume()
        assertEquals(1, commands.events.count { it == "tick" })
        clock.advance(50)
        controlled.decreaseDeviceVolume()
        assertEquals(1, commands.events.count { it == "tick" })
    }

    private fun controlledPlayer(
        player: Player = player(isPlaying = { true }),
        commands: CoreControlledPlayer.Commands = RecordingCommands(audio),
        now: () -> Long = { 0L },
    ) = CoreControlledPlayer(player, commands, context, now)

    private fun player(
        isPlaying: () -> Boolean,
        playWhenReady: () -> Boolean = { false },
        onAddListener: (Player.Listener) -> Unit = {},
    ): Player = Proxy.newProxyInstance(
        Player::class.java.classLoader,
        arrayOf(Player::class.java),
    ) { _, method, arguments ->
        when (method.name) {
            "isPlaying" -> isPlaying()
            "getPlayWhenReady" -> playWhenReady()
            "addListener" -> onAddListener(arguments?.first() as Player.Listener)
            "getAvailableCommands" -> Player.Commands.Builder().build()
            "isCommandAvailable" -> false
            else -> primitiveDefault(method.returnType)
        }
    } as Player
}

private class RecordingCommands(
    private val audio: AudioManager,
    private val enabled: () -> Boolean = { true },
) : CoreControlledPlayer.Commands {
    val events = mutableListOf<String>()

    override fun togglePause() = Unit

    override fun next() {
        events += "next:${audio.getStreamVolume(AudioManager.STREAM_MUSIC)}"
    }

    override fun previousInQueueOrder() {
        events += "previous:${audio.getStreamVolume(AudioManager.STREAM_MUSIC)}"
    }

    override fun isActivityInForeground(): Boolean = false

    override fun volumeKeyTrackSwitchEnabled(): Boolean = enabled()

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
