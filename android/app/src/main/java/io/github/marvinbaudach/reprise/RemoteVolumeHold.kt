package io.github.marvinbaudach.reprise

internal enum class VolumeDirection { UP, DOWN }

internal sealed interface RemoteVolumeAction {
    /** Apply one step to STREAM_MUSIC. */
    data class Step(val direction: VolumeDirection) : RemoteVolumeAction

    /** Restore STREAM_MUSIC to [restoreVolume], then skip, then tick. */
    data class Skip(
        val direction: VolumeDirection,
        val restoreVolume: Int,
    ) : RemoteVolumeAction

    /** This hold already skipped; swallow the rest of it. */
    data object Swallow : RemoteVolumeAction
}

internal class RemoteVolumeHold(
    private val repeatGapMaxMs: Long,
    private val leadInMaxMs: Long,
    private val isForeground: () -> Boolean,
    private val now: () -> Long,
) {
    private var previousTimeMs: Long? = null
    private var lastTimeMs: Long? = null
    private var lastDirection: VolumeDirection? = null
    private var skipped = false
    private var beforePrev: Int? = null
    private var beforeLast: Int? = null

    /** [currentVolume] is the live STREAM_MUSIC index before this call's step. */
    fun onAdjust(direction: VolumeDirection, currentVolume: Int): RemoteVolumeAction {
        val timeMs = now()
        val gapMs = lastTimeMs?.let { timeMs - it }
        val freshPress = lastDirection != direction || gapMs == null || gapMs > repeatGapMaxMs

        val action = when {
            freshPress -> {
                skipped = false
                recordStep(currentVolume)
                RemoteVolumeAction.Step(direction)
            }
            isForeground() -> {
                recordStep(currentVolume)
                RemoteVolumeAction.Step(direction)
            }
            skipped -> RemoteVolumeAction.Swallow
            else -> {
                skipped = true
                val previousGapMs = previousTimeMs?.let { previous ->
                    checkNotNull(lastTimeMs) - previous
                }
                val restoreVolume = if (
                    previousGapMs != null &&
                    previousGapMs <= leadInMaxMs &&
                    beforePrev != null
                ) {
                    checkNotNull(beforePrev)
                } else {
                    checkNotNull(beforeLast)
                }
                RemoteVolumeAction.Skip(direction, restoreVolume)
            }
        }

        previousTimeMs = lastTimeMs
        lastTimeMs = timeMs
        lastDirection = direction
        return action
    }

    private fun recordStep(currentVolume: Int) {
        beforePrev = beforeLast
        beforeLast = currentVolume
    }
}
