package de.reprise.spike

import android.content.Context
import android.os.Build
import android.os.VibrationEffect
import android.os.Vibrator
import android.os.VibratorManager
import android.provider.Settings
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.hapticfeedback.HapticFeedback
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalHapticFeedback

/**
 * The four things a queue reorder has to say through the case of the phone.
 *
 * They are named after the moment rather than after a waveform because the
 * waveform is not always available: a device without a vibrator, or a person
 * who has switched touch feedback off, gets the same gesture with nothing but
 * the animation, and neither is an error to report.
 */
internal interface QueueHaptics {
    /** The row has been picked up. */
    fun lift()

    /** The row has crossed into another slot. */
    fun crossedBoundary()

    /** The row has been let go somewhere new. */
    fun dropped()

    /** The row has been let go where it started, or the gesture was taken away. */
    fun cancelled()

    /** What a preview, a test, or a device without a vibrator uses. */
    object None : QueueHaptics {
        override fun lift() = Unit
        override fun crossedBoundary() = Unit
        override fun dropped() = Unit
        override fun cancelled() = Unit
    }
}

/**
 * The patterns from the drag-feel draft, in milliseconds.
 *
 * They are waveforms rather than [HapticFeedbackType] constants because two of
 * the four are not a single pulse: a drop is a firm tap followed by a lighter
 * echo, which is what tells a thumb the edit went through. Where no vibrator
 * answers, the nearest platform feedback stands in.
 */
private class DeviceQueueHaptics(
    private val context: Context,
    private val vibrator: Vibrator?,
    private val platform: HapticFeedback,
) : QueueHaptics {
    override fun lift() = pulse(longArrayOf(0, 18), HapticFeedbackType.LongPress)

    override fun crossedBoundary() = pulse(longArrayOf(0, 8), HapticFeedbackType.TextHandleMove)

    override fun dropped() = pulse(longArrayOf(0, 14, 40, 10), HapticFeedbackType.LongPress)

    override fun cancelled() = pulse(longArrayOf(0, 25), HapticFeedbackType.TextHandleMove)

    private fun pulse(pattern: LongArray, fallback: HapticFeedbackType) {
        // The platform route consults this setting on its own; the vibrator
        // route does not, so an app that reaches for it has to ask first.
        if (!touchFeedbackEnabled()) {
            return
        }
        val device = vibrator?.takeIf { it.hasVibrator() }
        if (device == null) {
            platform.performHapticFeedback(fallback)
            return
        }
        runCatching { device.vibrate(VibrationEffect.createWaveform(pattern, NO_REPEAT)) }
            .onFailure { platform.performHapticFeedback(fallback) }
    }

    // Deprecated with no replacement that an app can read: the supported route
    // is View.performHapticFeedback, which consults this same value internally
    // and is exactly the route the waveforms above cannot take.
    @Suppress("DEPRECATION")
    private fun touchFeedbackEnabled(): Boolean = runCatching {
        Settings.System.getInt(
            context.contentResolver,
            Settings.System.HAPTIC_FEEDBACK_ENABLED,
            1,
        ) != 0
    }.getOrDefault(true)

    private companion object {
        const val NO_REPEAT = -1
    }
}

@Composable
internal fun rememberQueueHaptics(): QueueHaptics {
    val context = LocalContext.current
    val platform = LocalHapticFeedback.current
    return remember(context, platform) {
        DeviceQueueHaptics(context, systemVibrator(context), platform)
    }
}

private fun systemVibrator(context: Context): Vibrator? = runCatching {
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
        context.getSystemService(VibratorManager::class.java)?.defaultVibrator
    } else {
        @Suppress("DEPRECATION")
        context.getSystemService(Vibrator::class.java)
    }
}.getOrNull()
