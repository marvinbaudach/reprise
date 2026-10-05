package io.github.marvinbaudach.reprise

/**
 * The transport as the library lists see it: what they ask to play never
 * includes a track that is about to be deleted.
 *
 * Several lists resolve their own ids — the artist page's Play asks the catalog
 * for every track the artist has — and hand them to [playTrackIds] without
 * knowing which of them a pending delete is hiding. Filtering there, once,
 * keeps those lists from queueing tracks the listener has already deleted.
 * Everything else is forwarded to [real] untouched.
 */
internal class VisibleTracksPlaybackControls(
    private val real: PlaybackControls,
    private val deletions: PendingDeletions,
) : PlaybackControls by real {
    override fun playTrackIds(trackIds: List<Long>, startIndex: Int) {
        val visible = deletions.withoutHidden(trackIds)
        if (visible.isEmpty() && trackIds.isNotEmpty()) {
            deletions.sayEverythingIsBeingDeleted()
            return
        }
        // The start stays on the same track, or on the next one that remains.
        val start = trackIds.take(startIndex).count { !deletions.isHidden(it) }
        real.playTrackIds(visible, start)
    }
}
