package io.github.marvinbaudach.reprise

import android.content.Context
import android.media.AudioManager
import android.os.SystemClock
import androidx.media3.common.DeviceInfo
import androidx.media3.common.ForwardingPlayer
import androidx.media3.common.Player
import java.util.concurrent.CopyOnWriteArraySet

/** Routes MediaSession transport commands back through the Core session. */
@Suppress("DEPRECATION")
internal class CoreControlledPlayer(
    player: Player,
    private val commands: Commands,
    context: Context,
    now: () -> Long = SystemClock::elapsedRealtime,
) : ForwardingPlayer(player) {
    private val audio = context.getSystemService(Context.AUDIO_SERVICE) as AudioManager
    private val listeners = CopyOnWriteArraySet<Player.Listener>()
    private val remoteVolumeHold = RemoteVolumeHold(
        repeatGapMaxMs = REPEAT_GAP_MAX_MS,
        leadInMaxMs = LEAD_IN_MAX_MS,
        isForeground = commands::isActivityInForeground,
        now = now,
    )
    private var publishedDeviceInfo = deviceInfo

    init {
        player.addListener(object : Player.Listener {
            override fun onIsPlayingChanged(isPlaying: Boolean) {
                refreshDeviceInfo()
            }
        })
    }

    internal interface Commands {
        fun togglePause()

        fun next()

        fun previousInQueueOrder()

        fun isActivityInForeground(): Boolean

        fun volumeKeyTrackSwitchEnabled(): Boolean

        fun hapticTick()
    }

    override fun addListener(listener: Player.Listener) {
        listeners += listener
        super.addListener(listener)
    }

    override fun removeListener(listener: Player.Listener) {
        listeners -= listener
        super.removeListener(listener)
    }

    override fun play() {
        if (!wrappedPlayer.playWhenReady) {
            commands.togglePause()
        }
    }

    override fun pause() {
        if (wrappedPlayer.playWhenReady) {
            commands.togglePause()
        }
    }

    override fun setPlayWhenReady(playWhenReady: Boolean) {
        if (playWhenReady != wrappedPlayer.playWhenReady) {
            commands.togglePause()
        }
    }

    override fun seekToNext() {
        commands.next()
    }

    override fun seekToNextMediaItem() {
        commands.next()
    }

    override fun seekToPrevious() {
        commands.previousInQueueOrder()
    }

    override fun seekToPreviousMediaItem() {
        commands.previousInQueueOrder()
    }

    override fun getDeviceInfo(): DeviceInfo = DeviceInfo.Builder(
        if (wrappedPlayer.isPlaying && commands.volumeKeyTrackSwitchEnabled()) {
            DeviceInfo.PLAYBACK_TYPE_REMOTE
        } else {
            DeviceInfo.PLAYBACK_TYPE_LOCAL
        },
    )
        .setMinVolume(0)
        .setMaxVolume(audio.getStreamMaxVolume(AudioManager.STREAM_MUSIC))
        .build()

    override fun getAvailableCommands(): Player.Commands = Player.Commands.Builder()
        .addAll(super.getAvailableCommands())
        .addAll(
            Player.COMMAND_GET_DEVICE_VOLUME,
            Player.COMMAND_SET_DEVICE_VOLUME,
            Player.COMMAND_SET_DEVICE_VOLUME_WITH_FLAGS,
            Player.COMMAND_ADJUST_DEVICE_VOLUME,
            Player.COMMAND_ADJUST_DEVICE_VOLUME_WITH_FLAGS,
        )
        .build()

    override fun isCommandAvailable(command: Int): Boolean = availableCommands.contains(command)

    override fun getDeviceVolume(): Int = audio.getStreamVolume(AudioManager.STREAM_MUSIC)

    override fun isDeviceMuted(): Boolean = audio.isStreamMute(AudioManager.STREAM_MUSIC)

    override fun increaseDeviceVolume() = increaseDeviceVolume(0)

    override fun increaseDeviceVolume(flags: Int) = adjustDeviceVolume(VolumeDirection.UP)

    override fun decreaseDeviceVolume() = decreaseDeviceVolume(0)

    override fun decreaseDeviceVolume(flags: Int) = adjustDeviceVolume(VolumeDirection.DOWN)

    override fun setDeviceVolume(volume: Int) = setDeviceVolume(volume, 0)

    override fun setDeviceVolume(volume: Int, flags: Int) {
        audio.setStreamVolume(AudioManager.STREAM_MUSIC, volume, flags)
        publishDeviceVolume()
    }

    internal fun refreshDeviceInfo() {
        val current = deviceInfo
        if (current == publishedDeviceInfo) return
        publishedDeviceInfo = current
        listeners.forEach { listener -> listener.onDeviceInfoChanged(current) }
    }

    private fun adjustDeviceVolume(direction: VolumeDirection) {
        val action = if (commands.volumeKeyTrackSwitchEnabled()) {
            remoteVolumeHold.onAdjust(direction, deviceVolume)
        } else {
            RemoteVolumeAction.Step(direction)
        }
        when (action) {
            is RemoteVolumeAction.Step -> {
                val adjustment = when (action.direction) {
                    VolumeDirection.UP -> AudioManager.ADJUST_RAISE
                    VolumeDirection.DOWN -> AudioManager.ADJUST_LOWER
                }
                audio.adjustStreamVolume(
                    AudioManager.STREAM_MUSIC,
                    adjustment,
                    AudioManager.FLAG_SHOW_UI,
                )
                publishDeviceVolume()
            }
            is RemoteVolumeAction.Skip -> {
                audio.setStreamVolume(AudioManager.STREAM_MUSIC, action.restoreVolume, 0)
                publishDeviceVolume()
                when (action.direction) {
                    VolumeDirection.UP -> commands.next()
                    VolumeDirection.DOWN -> commands.previousInQueueOrder()
                }
                commands.hapticTick()
            }
            RemoteVolumeAction.Swallow -> Unit
        }
    }

    private fun publishDeviceVolume() {
        val volume = deviceVolume
        val muted = isDeviceMuted
        listeners.forEach { listener -> listener.onDeviceVolumeChanged(volume, muted) }
    }

    private companion object {
        // Pixel 10 Pro XL: repeats arrived every 48-50 ms; the fastest tap gap was 156 ms.
        const val REPEAT_GAP_MAX_MS = 100L

        // Android's default long-press timeout, above the measured ~250 ms repeat lead-in.
        const val LEAD_IN_MAX_MS = 500L
    }
}
