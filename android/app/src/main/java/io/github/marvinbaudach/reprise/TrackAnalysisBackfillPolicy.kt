package io.github.marvinbaudach.reprise

/**
 * Whether the library-wide track-analysis backfill should be running right
 * now. Pure so [ReprisePlaybackService] can evaluate it on every playback
 * snapshot and only call `startTrackAnalysisBackfill`/
 * `cancelTrackAnalysisBackfill` on an actual transition.
 *
 * Decision 5 of `docs/plans/the-phone-analyses-its-own-music.md`: the rest of
 * the library is backfilled only while playback is [playing], and never
 * while [powerSaveMode] is on — there is no settings row for this, so battery
 * saver is the one signal a listener already controls.
 *
 * [playing] is play intent (playing or buffering). A `false` result is applied
 * by the service after `ANALYSIS_BACKFILL_STOP_GRACE_MS`, except for power-save
 * mode, which stops the backfill at once (#1129).
 */
internal fun analysisBackfillShouldRun(playing: Boolean, powerSaveMode: Boolean): Boolean =
    playing && !powerSaveMode
