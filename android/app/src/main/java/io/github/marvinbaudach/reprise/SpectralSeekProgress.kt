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
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.currentStateAsState
import kotlin.coroutines.resume
import kotlinx.coroutines.delay
import kotlinx.coroutines.suspendCancellableCoroutine

/** How often a surface without final analysis asks the running decode for more. */
internal const val ANALYSIS_PROGRESS_POLL_MS = 1_000L

/**
 * How many empty answers in a row end the polling until the next revision: half
 * a minute in which no decode of the track ran. A decode that starts later ends
 * with an import attempt, and the revision it bumps restarts the polling.
 */
internal const val MAX_EMPTY_PROGRESS_POLLS = 30

/**
 * The decoded part of the track's analysis while the phone is still computing
 * it. Polls while [active] and the screen is started, and answers `null` the
 * moment [active] turns false, so final data replaces the partial picture at
 * once. One read is outstanding at a time, an answer is applied only by the
 * loop that asked for it, and a track that keeps answering nothing stops being
 * asked until the next [revision].
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
    val lifecycleState by LocalLifecycleOwner.current.lifecycle.currentStateAsState()
    val polling = active && lifecycleState.isAtLeast(Lifecycle.State.STARTED)
    LaunchedEffect(analysis, trackId, count, revision, polling) {
        if (!polling) return@LaunchedEffect
        var seenThisRevision = false
        var emptyAnswers = 0
        while (emptyAnswers < MAX_EMPTY_PROGRESS_POLLS) {
            val answer = analysis.awaitProgress(trackId, count)
            if (answer != null) {
                seenThisRevision = true
                emptyAnswers = 0
                if (!answer.drawsLike(progress)) progress = answer
            } else {
                emptyAnswers += 1
                // Nothing in this revision: the decode is gone without a result. A null
                // after a partial in this same revision is the moment between the
                // decode's store and the revision bump that delivers the final data.
                if (!seenThisRevision) progress = null
            }
            delay(ANALYSIS_PROGRESS_POLL_MS)
        }
    }
    return if (active) progress else null
}

/**
 * One progress read, awaited: the next poll is not asked until this one is
 * answered, so slow reads never pile up on the read lane, and an answer that
 * arrives after the asking loop ended (a new revision, another track) is
 * dropped with its cancelled continuation.
 */
private suspend fun TrackAnalysisPort.awaitProgress(trackId: Long, count: Int) =
    suspendCancellableCoroutine { continuation ->
        loadProgress(trackId, count) { answer ->
            if (continuation.isActive) continuation.resume(answer)
        }
    }

/** Every poll delivers a new instance; one covering the same decoded part draws the same. */
private fun PartialTrackAnalysis.drawsLike(other: PartialTrackAnalysis?) =
    other != null &&
        coveredFraction == other.coveredFraction &&
        bars.size == other.bars.size &&
        frames.frameCount == other.frames.frameCount

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
