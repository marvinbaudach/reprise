package io.github.marvinbaudach.reprise

internal enum class VolumeDirection { UP, DOWN }

internal sealed interface RemoteVolumeAction {
    data class Step(val direction: VolumeDirection) : RemoteVolumeAction

    /** Restore STREAM_MUSIC to [restoreVolume], then skip in [direction], then tick. */
    data class Skip(
        val direction: VolumeDirection,
        val restoreVolume: Int,
    ) : RemoteVolumeAction
}

internal class RemoteVolumeGesture(
    private val rockMaxMs: Long,
    private val isForeground: () -> Boolean,
    private val now: () -> Long,
) {
    private data class FirstTap(
        val direction: VolumeDirection,
        val atMs: Long,
        val volume: Int,
    )

    private var firstTap: FirstTap? = null

    /** [currentVolume] is the live STREAM_MUSIC index before this call's step. */
    fun onAdjust(direction: VolumeDirection, currentVolume: Int): RemoteVolumeAction {
        val atMs = now()
        if (isForeground()) {
            firstTap = null
            return RemoteVolumeAction.Step(direction)
        }

        val first = firstTap
        if (
            first != null &&
            first.direction != direction &&
            atMs - first.atMs <= rockMaxMs
        ) {
            firstTap = null
            return RemoteVolumeAction.Skip(first.direction, restoreVolume = first.volume)
        }

        firstTap = FirstTap(direction, atMs, currentVolume)
        return RemoteVolumeAction.Step(direction)
    }
}
