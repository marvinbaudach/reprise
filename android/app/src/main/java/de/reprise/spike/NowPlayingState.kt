package de.reprise.spike

import uniffi.reprise_android_ffi.AndroidRepeatMode

internal data class NowPlayingMetrics(
    val coverSizeDp: Int,
    val coverRadiusDp: Int,
    val titleSizeSp: Int,
    val titleLineHeightSp: Int,
    val artistSizeSp: Int,
    val artistLineHeightSp: Int,
)

internal val nowPlayingMetrics = NowPlayingMetrics(
    coverSizeDp = 364,
    coverRadiusDp = 28,
    titleSizeSp = 28,
    titleLineHeightSp = 36,
    artistSizeSp = 16,
    artistLineHeightSp = 24,
)

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
