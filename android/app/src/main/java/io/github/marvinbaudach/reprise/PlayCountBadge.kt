package io.github.marvinbaudach.reprise

import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.unit.dp

private const val MAX_FORMATTED_PLAY_COUNT = 999_499_999_999L
private const val THOUSAND = 1_000L
private const val MILLION = 1_000_000L
private const val BILLION = 1_000_000_000L
private const val TENTHS_ROLLOVER = 100L

internal fun formatPlayCount(count: Long): String {
    val normalized = count.coerceIn(0L, MAX_FORMATTED_PLAY_COUNT)
    if (normalized < THOUSAND) return normalized.toString()

    val (unit, suffix, nextSuffix) = when {
        normalized >= BILLION -> CountUnit(BILLION, "B", "B")
        normalized >= MILLION -> CountUnit(MILLION, "M", "B")
        else -> CountUnit(THOUSAND, "k", "M")
    }
    if (normalized < 10 * unit) {
        val tenths = (normalized + unit / 20) / (unit / 10)
        if (tenths >= TENTHS_ROLLOVER) return "10$suffix"
        val decimal = tenths % 10
        return if (decimal == 0L) {
            "${tenths / 10}$suffix"
        } else {
            "${tenths / 10}.$decimal$suffix"
        }
    }

    val whole = (normalized + unit / 2) / unit
    return if (whole >= THOUSAND) "1$nextSuffix" else "$whole$suffix"
}

private data class CountUnit(
    val value: Long,
    val suffix: String,
    val nextSuffix: String,
)

@Composable
internal fun PlayCountBadge(playCount: Long) {
    val normalizedPlayCount = playCount.coerceAtLeast(0)
    val description = pluralStringResource(
        R.plurals.play_count_description,
        normalizedPlayCount.coerceAtMost(Int.MAX_VALUE.toLong()).toInt(),
        normalizedPlayCount,
    )
    // A never-played track keeps the real badge invisible and silent so the duration
    // stays aligned; unlike a fixed-height box, this slot follows font scaling.
    val modifier = if (normalizedPlayCount == 0L) {
        Modifier
            .alpha(0f)
            .clearAndSetSemantics {}
    } else {
        Modifier.clearAndSetSemantics { contentDescription = description }
    }
    Surface(
        modifier = modifier,
        color = MaterialTheme.colorScheme.secondaryContainer,
        contentColor = MaterialTheme.colorScheme.onSecondaryContainer,
        shape = MaterialTheme.shapes.small,
    ) {
        Row(
            modifier = Modifier.padding(horizontal = 5.dp, vertical = 1.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            MaterialSymbol("play_arrow", "", sizeSp = 12)
            Text(
                text = formatPlayCount(normalizedPlayCount),
                modifier = Modifier.clearAndSetSemantics {},
                style = MaterialTheme.typography.labelSmall,
            )
        }
    }
}
