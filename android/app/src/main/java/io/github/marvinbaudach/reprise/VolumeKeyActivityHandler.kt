package io.github.marvinbaudach.reprise

import android.content.Context
import android.media.AudioManager
import android.view.KeyEvent

/** Android-framework wiring for the pure [VolumeKeyTrackSwitch] decision. */
internal class VolumeKeyActivityHandler(
    context: Context,
    isPlaying: () -> Boolean,
) {
    private val trackSwitch = VolumeKeyTrackSwitch(isPlaying)
    private val audioManager by lazy { context.getSystemService(AudioManager::class.java) }
    private var surfaceControls: PlaybackControls? = null

    /** Keep transport commands on the same injected controls used by the rendered surface. */
    fun useSurfaceControls(controls: PlaybackControls) {
        surfaceControls = controls
    }

    fun onDown(keyCode: Int, event: KeyEvent): Boolean? {
        val key = volumeKey(keyCode) ?: return null
        return when (trackSwitch.onDown(key, event.repeatCount == 0)) {
            VolumeKeyAction.StartTracking -> {
                event.startTracking()
                true
            }
            VolumeKeyAction.Ignore -> true
            VolumeKeyAction.Passthrough -> null
            else -> null
        }
    }

    fun onLongPress(keyCode: Int): Boolean? {
        val key = volumeKey(keyCode) ?: return null
        return when (trackSwitch.onLongPress(key)) {
            VolumeKeyAction.SkipNext -> {
                surfaceControls?.next()
                true
            }
            VolumeKeyAction.SkipPrevious -> {
                surfaceControls?.previous()
                true
            }
            VolumeKeyAction.Ignore -> true
            VolumeKeyAction.Passthrough -> null
            else -> null
        }
    }

    fun onUp(keyCode: Int, event: KeyEvent): Boolean? {
        val key = volumeKey(keyCode) ?: return null
        return when (
            val action = trackSwitch.onUp(
                key,
                wasTracking = event.isTracking(),
                wasCanceled = event.isCanceled(),
            )
        ) {
            is VolumeKeyAction.AdjustVolume -> {
                audioManager.adjustStreamVolume(
                    AudioManager.STREAM_MUSIC,
                    action.key.adjustDirection(),
                    AudioManager.FLAG_SHOW_UI,
                )
                true
            }
            VolumeKeyAction.Ignore -> true
            VolumeKeyAction.Passthrough -> null
            else -> null
        }
    }

    /** Drop a press whose up event will not return to this activity. */
    fun forget() = trackSwitch.forget()
}

private fun volumeKey(keyCode: Int): VolumeKey? = when (keyCode) {
    KeyEvent.KEYCODE_VOLUME_UP -> VolumeKey.UP
    KeyEvent.KEYCODE_VOLUME_DOWN -> VolumeKey.DOWN
    else -> null
}

private fun VolumeKey.adjustDirection(): Int = when (this) {
    VolumeKey.UP -> AudioManager.ADJUST_RAISE
    VolumeKey.DOWN -> AudioManager.ADJUST_LOWER
}
