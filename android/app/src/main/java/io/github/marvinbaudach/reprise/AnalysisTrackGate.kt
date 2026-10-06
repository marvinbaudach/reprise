package io.github.marvinbaudach.reprise

/**
 * The playing service's current analysis track, and the one place a
 * supersede is decided against it.
 *
 * Deciding and acting happen under one lock, and the track only moves under
 * that lock too. Without it, a call keeping B could pass its check, lose the
 * thread while playback moved on to C and C's analysis started, and then
 * cancel C — the track now playing. The library call made under the lock only
 * flips flags and joins nothing, so a track change waits microseconds at most;
 * the caller resolves the library before it enters.
 */
internal class AnalysisTrackGate {
    private val lock = Any()

    @Volatile
    var current: Long? = null
        private set

    /**
     * The last track playback moved to. A stop clears [current] but keeps this:
     * the stopped track's analysis is left to finish, and it is still the track
     * every supersede queued before the stop is measured against.
     */
    @Volatile
    private var latest: Long? = null

    fun moveTo(trackId: Long?) {
        synchronized(lock) {
            current = trackId
            if (trackId != null) latest = trackId
        }
    }

    /**
     * Whether an analysis request for [trackId] still belongs to the track
     * playback last moved to. A stop does not take it away; a switch to another
     * track does, even when a stop follows the switch.
     */
    fun stillWanted(trackId: Long): Boolean = latest == trackId

    /**
     * Runs [supersede] for [keepTrackId] unless playback has since moved to
     * another track, whose own call is queued behind this one. A stop after
     * the switch still supersedes: nothing newer will stop the outgoing
     * track's decode. A stop after a further switch does not — the call
     * keeping the last track does that, and the last track is left to finish.
     * Returns whether [supersede] ran.
     */
    fun supersedeOthers(keepTrackId: Long, supersede: (Long) -> Unit): Boolean =
        synchronized(lock) {
            if (keepTrackId != latest) return false
            supersede(keepTrackId)
            true
        }
}
