package io.github.marvinbaudach.reprise

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class PlayCountFormatTest {
    @Test
    fun formatsCountsAtUnitAndRoundingBoundaries() {
        val cases = listOf(
            7L to "7",
            999L to "999",
            1_000L to "1k",
            1_049L to "1k",
            1_050L to "1.1k",
            1_234L to "1.2k",
            1_949L to "1.9k",
            1_950L to "2k",
            9_949L to "9.9k",
            9_950L to "10k",
            12_345L to "12k",
            12_500L to "13k",
            99_500L to "100k",
            999_499L to "999k",
            999_500L to "1M",
            1_250_000L to "1.3M",
            999_499_999L to "999M",
            999_500_000L to "1B",
            Long.MAX_VALUE to "999B",
            -5L to "0",
        )

        cases.forEach { (count, expected) ->
            assertEquals("count $count", expected, formatPlayCount(count))
        }
    }

    @Test
    fun formattedCountsNeverExceedFourCharacters() {
        playCountLengthSweep().forEach { count ->
            val formatted = formatPlayCount(count)
            assertTrue(
                "count $count formatted as '$formatted'",
                formatted.length <= 4,
            )
        }
    }

    private fun playCountLengthSweep(): Set<Long> = buildSet {
        add(0)
        add(Long.MAX_VALUE)
        var power = 1L
        while (power > 0) {
            for (offset in -1L..1L) {
                add((power + offset).coerceAtLeast(0))
            }
            if (power >= 1_000) {
                listOf(51L, 50L, 501L, 500L).forEach { distance ->
                    add(power - distance)
                }
            }
            if (power > Long.MAX_VALUE / 10) break
            power *= 10
        }
    }
}
