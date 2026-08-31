package io.github.marvinbaudach.reprise

/** What a tap is told when the writer has already stopped. */
internal const val RATING_WRITER_STOPPED = "the library is closing"

/**
 * The activity's rating writer: every heart tap uses the shared write lane.
 *
 * A rating is a SQLite `UPDATE` behind the same handle a SAF scan holds for the
 * whole of its folder walk, so writing it where the tap happens puts the main
 * thread behind a transaction of unbounded length. Play counting was moved off
 * Media3's application thread for the same reason (`play_recorder.rs`); this is
 * the discrete-action half of it.
 *
 * What deliberately does **not** change is *when* the heart moves. This is not
 * fire-and-forget: every tap is answered, exactly once, through [setFavourite]'s
 * `report` — and [LibraryWrites] delivers the answer on the main thread so the
 * caller may write Compose state from it directly. A tap that cannot even be
 * queued is answered too, because a heart that neither moves nor explains
 * itself is worse than a slow one.
 *
 * One thread rather than a pool is load-bearing: quick toggles reach the
 * database in order, so the final favourite state is the last one tapped.
 */
internal class RatingWriter(
    private val write: (Long, Boolean) -> Unit,
    private val libraryWrites: LibraryWrites,
) {
    /**
     * Queues one rating and answers [report] on the main thread — with the
     * failure the write raised, or success once the database has agreed.
     */
    fun setFavourite(trackId: Long, favourite: Boolean, report: (Result<Unit>) -> Unit) {
        libraryWrites.submitAnswered(
            work = { write(trackId, favourite) },
            report = report,
        )
    }
}
