package io.github.marvinbaudach.reprise

import uniffi.reprise_android_ffi.AndroidAnalysisOutcome

internal const val MAX_ANALYSIS_ATTEMPTS = 3

internal fun trackAnalysisIsNonFinal(
    outcome: AndroidAnalysisOutcome?,
    error: Throwable? = null,
): Boolean =
    error != null ||
        outcome == AndroidAnalysisOutcome.CANCELLED ||
        outcome == AndroidAnalysisOutcome.PHONE_SOURCE_CHANGED

/**
 * Whether an import attempt should be made again. `SUPERSEDED` is final for a
 * track nobody plays any more, but the track that is still playing can only
 * have been reached by a stale supersede, so it is retried like a cancel.
 */
internal fun trackAnalysisShouldRetry(
    outcome: AndroidAnalysisOutcome?,
    error: Throwable?,
    stillPlaying: Boolean,
): Boolean =
    trackAnalysisIsNonFinal(outcome, error) ||
        stillPlaying && outcome == AndroidAnalysisOutcome.SUPERSEDED
