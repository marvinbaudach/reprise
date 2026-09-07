package io.github.marvinbaudach.reprise

import java.util.Locale
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * Pins `formatDuration` to the same contract the desktop asserts.
 *
 * The rule "a duration reads `m:ss`, or `h:mm:ss` past the hour" is written out
 * twice — once in Kotlin, once in `crates/reprise-core/src/format.rs` — because
 * formatting a list row through the FFI would cost a JNI crossing per row per
 * frame.
 *
 * Two copies need a mechanism rather than a comment. The mechanism is
 * `scripts/check-duration-format-parity.sh`: it extracts the asserted
 * input/output pairs from both files and fails the architecture gate when a
 * case Rust pins is absent or different here. The first block below is
 * therefore not decoration — it is the set that script compares against. The
 * later blocks are extra coverage, which the script permits.
 *
 * The hour cases are not academic: `BrowseTabs` formats a whole album's total
 * duration with this function, and podcast episodes routinely run past an hour.
 * Before this test, both rendered a 62-minute value as "62:33".
 */
class DurationFormatTest {
    private val originalLocale: Locale = Locale.getDefault()

    @After
    fun restoreLocale() {
        Locale.setDefault(originalLocale)
    }

    @Test
    fun matchesTheCasesTheRustSideAsserts() {
        // Mirrors crates/reprise-core/src/format.rs.
        assertEquals("3:01", formatDuration(181_000))
        assertEquals("0:59", formatDuration(59_000))
        assertEquals("1:02:33", formatDuration(3_753_000))
        assertEquals("0:00", formatDuration(-5))
        assertEquals("0:00", formatDuration(0))
    }

    @Test
    fun crossesIntoHoursExactlyAtTheHourMark() {
        assertEquals("59:59", formatDuration(3_599_000))
        assertEquals("1:00:00", formatDuration(3_600_000))
        assertEquals("1:00:01", formatDuration(3_601_000))
    }

    @Test
    fun keepsLongAlbumTotalsReadable() {
        // A 74-minute album, the case that used to read "74:00".
        assertEquals("1:14:00", formatDuration(74 * 60 * 1_000L))
        // A box set well past the day of an ordinary track.
        assertEquals("5:12:00", formatDuration(312 * 60 * 1_000L))
    }

    @Test
    fun rendersAsciiDigitsRegardlessOfDeviceLocale() {
        // The desktop emits ASCII; a locale-sensitive format would show the same
        // track differently on the phone.
        Locale.setDefault(Locale.forLanguageTag("ar-EG-u-nu-arab"))
        assertEquals("3:01", formatDuration(181_000))
        assertEquals("1:02:33", formatDuration(3_753_000))
    }
}
