package io.github.marvinbaudach.reprise

internal enum class VolumeKey { UP, DOWN }

internal sealed interface VolumeKeyAction {
    /** Consume the event and ask the framework to time a long press. */
    data object StartTracking : VolumeKeyAction

    data object SkipNext : VolumeKeyAction

    data object SkipPrevious : VolumeKeyAction

    /** Consume it and apply the one volume step the framework no longer applies. */
    data class AdjustVolume(val key: VolumeKey) : VolumeKeyAction

    /** Consume it and do nothing. */
    data object Ignore : VolumeKeyAction

    /** Do not consume it — the system handles the key. */
    data object Passthrough : VolumeKeyAction
}

internal class VolumeKeyTrackSwitch(private val isPlaying: () -> Boolean) {
    private var consumed: VolumeKey? = null

    fun onDown(key: VolumeKey, isFirstPress: Boolean): VolumeKeyAction {
        if (!isFirstPress) {
            return if (consumed == key) VolumeKeyAction.Ignore else VolumeKeyAction.Passthrough
        }
        if (!isPlaying()) {
            consumed = null
            return VolumeKeyAction.Passthrough
        }

        // If both keys are held, the most recent press owns the gesture; the
        // earlier key's eventual up event falls through to the system.
        consumed = key
        return VolumeKeyAction.StartTracking
    }

    fun onLongPress(key: VolumeKey): VolumeKeyAction {
        if (consumed != key) return VolumeKeyAction.Passthrough
        return when (key) {
            VolumeKey.UP -> VolumeKeyAction.SkipNext
            VolumeKey.DOWN -> VolumeKeyAction.SkipPrevious
        }
    }

    fun onUp(
        key: VolumeKey,
        wasTracking: Boolean,
        wasCanceled: Boolean,
    ): VolumeKeyAction {
        if (consumed != key) return VolumeKeyAction.Passthrough
        consumed = null
        return if (wasTracking && !wasCanceled) {
            VolumeKeyAction.AdjustVolume(key)
        } else {
            VolumeKeyAction.Ignore
        }
    }

    /** Drop a press the activity will never see the end of. */
    fun forget() {
        consumed = null
    }
}
