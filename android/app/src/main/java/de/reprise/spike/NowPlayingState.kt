package de.reprise.spike

import uniffi.reprise_android_ffi.AndroidRepeatMode

internal data class NowPlayingMetrics(
    val coverSizeDp: Int,
    val coverRadiusDp: Int,
    val titleSizeSp: Int,
    val titleLineHeightSp: Int,
    val artistSizeSp: Int,
    val artistLineHeightSp: Int,
    val playButtonSizeDp: Int,
    val playButtonRadiusDp: Int,
)

internal val nowPlayingMetrics = NowPlayingMetrics(
    coverSizeDp = 364,
    coverRadiusDp = 28,
    titleSizeSp = 28,
    titleLineHeightSp = 36,
    artistSizeSp = 16,
    artistLineHeightSp = 24,
    // The sheet's play button is the frame's largest control, and 28 dp is the
    // shape scale's top rung — a rounded square, not a circle. The radius lives
    // here so the test can hold the theme's `extraLarge` to it.
    playButtonSizeDp = 80,
    playButtonRadiusDp = 28,
)

/** How long a rating failure stays readable before it dismisses itself. */
internal const val RATING_FAILURE_MS = 4_000L

/**
 * A message that has to outlive the next 500 ms playback snapshot and then
 * leave on its own.
 *
 * [occurrence] is what makes the same text raised twice a *new* event: without
 * it the dismissal timer would still be running on the first one's schedule and
 * the second message would vanish early.
 */
internal data class TransientMessage(val text: String, val occurrence: Long = 0) {
    fun after(previous: TransientMessage?): TransientMessage =
        if (previous == null) this else copy(occurrence = previous.occurrence + 1)
}

/**
 * The head's own reading of what is left, as `−m:ss`. The total belongs to the
 * track; the sheet is about where the playhead is.
 */
internal fun formatRemaining(positionMs: Long, durationMs: Long): String =
    if (durationMs > 0) {
        "−${formatDuration((durationMs - positionMs).coerceAtLeast(0))}"
    } else {
        "--:--"
    }

internal fun cycleRepeatMode(mode: AndroidRepeatMode): AndroidRepeatMode = when (mode) {
    AndroidRepeatMode.OFF -> AndroidRepeatMode.ALL
    AndroidRepeatMode.ALL -> AndroidRepeatMode.ONE
    AndroidRepeatMode.ONE -> AndroidRepeatMode.OFF
}

/**
 * Displayed seek position and its current owner.
 *
 * Snapshot ticks own the head while idle. The finger owns it from the first
 * drag value through release, so a 500 ms Media3 tick cannot pull the head
 * backwards underneath an in-flight gesture.
 */
internal data class SeekPositionState(
    val positionMs: Long,
    val isDragging: Boolean,
) {
    fun acceptSnapshot(positionMs: Long): SeekPositionState =
        if (isDragging) this else fromSnapshot(positionMs)

    fun dragTo(positionMs: Long): SeekPositionState = SeekPositionState(
        positionMs = positionMs.coerceAtLeast(0),
        isDragging = true,
    )

    fun release(): SeekPositionState = copy(isDragging = false)

    companion object {
        fun fromSnapshot(positionMs: Long): SeekPositionState = SeekPositionState(
            positionMs = positionMs.coerceAtLeast(0),
            isDragging = false,
        )
    }
}
