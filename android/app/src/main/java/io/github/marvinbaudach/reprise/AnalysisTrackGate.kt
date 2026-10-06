package io.github.marvinbaudach.reprise

/**
 * The playing service's current analysis track, and the one place a
 * supersede is decided against it.
 *
 * Deciding and acting happen under one lock, and the track only moves under
 * that lock too. Without it, a call keeping B could pass its check, lose the
 * thread while playback moved on to C and C's analysis started, and then
 * cancel C — the track now playing. The library call made under the lock only
 * flips flags and joins nothing, so a track change waits microseconds at most.
 */
internal class AnalysisTrackGate {
    private val lock = Any()

    @Volatile
    var current: Long? = null
        private set

    fun moveTo(trackId: Long?) {
        synchronized(lock) { current = trackId }
    }

    /**
     * Runs [supersede] for [keepTrackId] unless playback has since moved to
     * another track, whose own call is queued behind this one. A stop after
     * the switch (no current track) still supersedes: nothing newer will stop
     * the outgoing track's decode. Returns whether [supersede] ran.
     */
    fun supersedeOthers(keepTrackId: Long, supersede: (Long) -> Unit): Boolean =
        synchronized(lock) {
            val now = current
            if (now != null && now != keepTrackId) return false
            supersede(keepTrackId)
            true
        }
}
