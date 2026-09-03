package io.github.marvinbaudach.reprise

import android.content.Context
import android.media.AudioManager
import android.util.Log
import androidx.media3.common.DeviceInfo
import androidx.media3.common.ForwardingPlayer
import androidx.media3.common.Player

/** Routes MediaSession transport commands back through the Core session. */
@Suppress("DEPRECATION", "OVERRIDE_DEPRECATION")
internal class CoreControlledPlayer(
    player: Player,
    private val commands: Commands,
    context: Context? = null,
) : ForwardingPlayer(player) {
    // VOLUME-KEY REMOTE-TARGET SPIKE ONLY. Remove this entire device-volume
    // surface after the Pixel measurement; it is an instrument, not a feature.
    private val audioManager: AudioManager by lazy {
        checkNotNull(context) { "The volume spike requires an Android Context" }
            .getSystemService(AudioManager::class.java)
    }

    internal interface Commands {
        fun togglePause()

        fun next()

        fun previousInQueueOrder()
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

    override fun getDeviceInfo(): DeviceInfo {
        val maxVolume = audioManager.getStreamMaxVolume(AudioManager.STREAM_MUSIC)
        Log.i(VOLUME_SPIKE_LOG_TAG, "getDeviceInfo maxVolume=$maxVolume nanos=${System.nanoTime()}")
        return DeviceInfo(DeviceInfo.PLAYBACK_TYPE_REMOTE, 0, maxVolume)
    }

    override fun getAvailableCommands(): Player.Commands =
        super.getAvailableCommands().buildUpon()
            .add(Player.COMMAND_GET_DEVICE_VOLUME)
            .add(Player.COMMAND_SET_DEVICE_VOLUME)
            .add(Player.COMMAND_ADJUST_DEVICE_VOLUME)
            .add(Player.COMMAND_SET_DEVICE_VOLUME_WITH_FLAGS)
            .add(Player.COMMAND_ADJUST_DEVICE_VOLUME_WITH_FLAGS)
            .build()

    // NOT part of the spike surface: this override is the fix, and it outlives
    // the instrumentation above it. `ForwardingPlayer.isCommandAvailable(int)`
    // does not consult `getAvailableCommands()` — it asks the wrapped player,
    // which plays to local output and answers false for the device-volume
    // commands. Media3's volume provider asks exactly that method and drops the
    // adjust silently, so none of the overrides below were ever reached.
    // Derived from `getAvailableCommands()` rather than a hand-written list so
    // the two answers cannot drift apart, which is the failure being fixed.
    override fun isCommandAvailable(command: Int): Boolean {
        val available = getAvailableCommands().contains(command)
        if (command in DEVICE_VOLUME_ADJUST_COMMANDS) {
            Log.i(
                VOLUME_SPIKE_LOG_TAG,
                "isCommandAvailable command=$command available=$available " +
                    "nanos=${System.nanoTime()}",
            )
        }
        return available
    }

    override fun getDeviceVolume(): Int {
        val volume = audioManager.getStreamVolume(AudioManager.STREAM_MUSIC)
        Log.i(VOLUME_SPIKE_LOG_TAG, "getDeviceVolume volume=$volume nanos=${System.nanoTime()}")
        return volume
    }

    override fun isDeviceMuted(): Boolean {
        val muted = audioManager.isStreamMute(AudioManager.STREAM_MUSIC)
        Log.i(VOLUME_SPIKE_LOG_TAG, "isDeviceMuted muted=$muted nanos=${System.nanoTime()}")
        return muted
    }

    override fun increaseDeviceVolume() {
        Log.i(VOLUME_SPIKE_LOG_TAG, "increaseDeviceVolume nanos=${System.nanoTime()}")
        adjustMusicStream(AudioManager.ADJUST_RAISE)
    }

    override fun increaseDeviceVolume(flags: Int) {
        Log.i(
            VOLUME_SPIKE_LOG_TAG,
            "increaseDeviceVolume flags=$flags nanos=${System.nanoTime()}",
        )
        adjustMusicStream(AudioManager.ADJUST_RAISE)
    }

    override fun decreaseDeviceVolume() {
        Log.i(VOLUME_SPIKE_LOG_TAG, "decreaseDeviceVolume nanos=${System.nanoTime()}")
        adjustMusicStream(AudioManager.ADJUST_LOWER)
    }

    override fun decreaseDeviceVolume(flags: Int) {
        Log.i(
            VOLUME_SPIKE_LOG_TAG,
            "decreaseDeviceVolume flags=$flags nanos=${System.nanoTime()}",
        )
        adjustMusicStream(AudioManager.ADJUST_LOWER)
    }

    override fun setDeviceVolume(volume: Int) {
        Log.i(VOLUME_SPIKE_LOG_TAG, "setDeviceVolume volume=$volume nanos=${System.nanoTime()}")
        setMusicStreamVolume(volume)
    }

    override fun setDeviceVolume(volume: Int, flags: Int) {
        Log.i(
            VOLUME_SPIKE_LOG_TAG,
            "setDeviceVolume volume=$volume flags=$flags nanos=${System.nanoTime()}",
        )
        setMusicStreamVolume(volume)
    }

    override fun setDeviceMuted(muted: Boolean) {
        Log.i(VOLUME_SPIKE_LOG_TAG, "setDeviceMuted muted=$muted nanos=${System.nanoTime()}")
        setMusicStreamMuted(muted)
    }

    override fun setDeviceMuted(muted: Boolean, flags: Int) {
        Log.i(
            VOLUME_SPIKE_LOG_TAG,
            "setDeviceMuted muted=$muted flags=$flags nanos=${System.nanoTime()}",
        )
        setMusicStreamMuted(muted)
    }

    private fun adjustMusicStream(direction: Int) {
        audioManager.adjustStreamVolume(
            AudioManager.STREAM_MUSIC,
            direction,
            AudioManager.FLAG_SHOW_UI,
        )
    }

    private fun setMusicStreamVolume(volume: Int) {
        audioManager.setStreamVolume(
            AudioManager.STREAM_MUSIC,
            volume,
            AudioManager.FLAG_SHOW_UI,
        )
    }

    private fun setMusicStreamMuted(muted: Boolean) {
        adjustMusicStream(if (muted) AudioManager.ADJUST_MUTE else AudioManager.ADJUST_UNMUTE)
    }

    private companion object {
        const val VOLUME_SPIKE_LOG_TAG = "VolSpike"

        // The two commands Media3's volume provider guards the adjust with;
        // logging only these keeps the fix's evidence out of a flood, because
        // `isCommandAvailable` is asked for every command in many places.
        val DEVICE_VOLUME_ADJUST_COMMANDS = setOf(
            Player.COMMAND_ADJUST_DEVICE_VOLUME,
            Player.COMMAND_ADJUST_DEVICE_VOLUME_WITH_FLAGS,
        )
    }
}
