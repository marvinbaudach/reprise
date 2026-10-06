package io.github.marvinbaudach.reprise

import android.content.Context
import android.media.AudioManager
import android.os.SystemClock
import android.util.Log
import androidx.media3.common.DeviceInfo
import androidx.media3.common.ForwardingPlayer
import androidx.media3.common.Player
import androidx.media3.common.util.UnstableApi
import java.util.concurrent.CopyOnWriteArraySet

/** Routes MediaSession transport commands back through the Core session. */
// ForwardingPlayer and its overrides are unstable in media3 1.11; one opt-in
// covers the class instead of a baseline entry per override.
@androidx.annotation.OptIn(UnstableApi::class)
@Suppress("DEPRECATION", "OVERRIDE_DEPRECATION")
internal class CoreControlledPlayer(
    player: Player,
    private val commands: Commands,
    context: Context,
    private val now: () -> Long = SystemClock::elapsedRealtime,
) : ForwardingPlayer(player) {
    private val audio = context.getSystemService(Context.AUDIO_SERVICE) as AudioManager
    private val listeners = CopyOnWriteArraySet<Player.Listener>()
    private val remoteVolumeGesture = RemoteVolumeGesture(
        rockMaxMs = ROCK_MAX_MS,
        isForeground = commands::isActivityInForeground,
        now = now,
    )
    private var publishedDeviceInfo = deviceInfo
    private var previousVolumeCallbackAt: Long? = null

    init {
        player.addListener(object : Player.Listener {
            override fun onPlayWhenReadyChanged(playWhenReady: Boolean, reason: Int) {
                refreshDeviceInfo()
            }

            override fun onPlaybackStateChanged(playbackState: Int) {
                refreshDeviceInfo()
            }
        })
    }

    internal interface Commands {
        fun togglePause()

        fun next()

        fun previousInQueueOrder()

        fun isActivityInForeground(): Boolean

        fun volumeKeySkipGestureEnabled(): Boolean

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
        if (
            wrappedPlayer.playWhenReady &&
            wrappedPlayer.playbackState.isRemotePlaybackState() &&
            commands.volumeKeySkipGestureEnabled()
        ) {
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
        val callbackAt = now()
        val gapMs = previousVolumeCallbackAt?.let { callbackAt - it }
        previousVolumeCallbackAt = callbackAt
        val currentVolume = deviceVolume
        val action = if (commands.volumeKeySkipGestureEnabled()) {
            remoteVolumeGesture.onAdjust(direction, currentVolume)
        } else {
            RemoteVolumeAction.Step(direction)
        }
        if (BuildConfig.DEBUG) {
            Log.d(
                VOLUME_KEY_LOG_TAG,
                "direction=$direction gapMs=${gapMs ?: "none"} volume=$currentVolume action=$action",
            )
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
        }
    }

    private fun publishDeviceVolume() {
        val volume = deviceVolume
        val muted = isDeviceMuted
        listeners.forEach { listener -> listener.onDeviceVolumeChanged(volume, muted) }
    }

    private companion object {
        const val VOLUME_KEY_LOG_TAG = "VolumeKeys"

        // Pixel 10 Pro XL: quick single taps were 309-490 ms apart; the opposite
        // second direction, not the gap alone, distinguishes the rock.
        const val ROCK_MAX_MS = 500L
    }
}

private fun Int.isRemotePlaybackState(): Boolean =
    this == Player.STATE_READY || this == Player.STATE_BUFFERING
