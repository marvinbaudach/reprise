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
