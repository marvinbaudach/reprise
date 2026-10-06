package io.github.marvinbaudach.reprise

import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive

/** How often a surface without final analysis asks the running decode for more. */
internal const val ANALYSIS_PROGRESS_POLL_MS = 1_000L

/**
 * The decoded part of the track's analysis while the phone is still computing
 * it. Polls while [active] and answers `null` the moment it is not, so final
 * data replaces the partial picture at once. The loop ends when the composable
 * leaves composition or [active] turns false.
 */
@Composable
internal fun rememberAnalysisProgress(
    analysis: TrackAnalysisPort,
    trackId: Long,
    count: Int,
    revision: Long,
    active: Boolean,
): PartialTrackAnalysis? {
    var progress by remember(trackId, count) { mutableStateOf<PartialTrackAnalysis?>(null) }
    LaunchedEffect(analysis, trackId, count, revision, active) {
        if (!active) return@LaunchedEffect
        while (isActive) {
            analysis.loadProgress(trackId, count) { answer -> progress = answer }
            delay(ANALYSIS_PROGRESS_POLL_MS)
        }
    }
    return if (active) progress else null
}

/**
 * The plain seek line for everything right of [coveredWidth]: the undecoded
 * part of the track, or the whole track when nothing is decoded.
 */
internal fun DrawScope.drawPlainSeekLine(
    coveredWidth: Float,
    fraction: Float,
    elapsed: Color,
    remaining: Color,
) {
    val centre = size.height / 2f
    val thickness = SEEK_TRACK_THICKNESS_DP.dp.toPx()
    val head = size.width * fraction
    drawLine(
        color = remaining,
        start = Offset(maxOf(head, coveredWidth), centre),
        end = Offset(size.width, centre),
        strokeWidth = thickness,
        cap = StrokeCap.Round,
    )
    if (head > coveredWidth) {
        drawLine(
            color = elapsed,
            start = Offset(coveredWidth, centre),
            end = Offset(head, centre),
            strokeWidth = thickness,
            cap = StrokeCap.Round,
        )
    }
}
