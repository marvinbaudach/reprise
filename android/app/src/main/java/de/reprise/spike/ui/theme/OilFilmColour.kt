package de.reprise.spike.ui.theme

import androidx.compose.ui.graphics.Color

/**
 * Builds one of the oil film's colours from channels it has already computed.
 *
 * The film derives its palette from the artwork — quadrant means, a lift for
 * dark covers, a saturate and a contrast pass — and none of that arithmetic
 * belongs to the theme. The resulting Compose colour does. Every colour this
 * app draws with is constructed under this directory, which is what
 * `check-android-theme.sh` reads the sources to enforce, so the film hands its
 * channels here rather than reaching for the constructor itself.
 *
 * Channels arrive already in range on every path that calls this, and are
 * clamped anyway: a colour built out of range throws, and a haze behind the
 * cover is not worth a crash.
 */
internal fun oilFilmColour(
    red: Float,
    green: Float,
    blue: Float,
    alpha: Float = 1f,
): Color = Color(
    red = red.coerceIn(0f, 1f),
    green = green.coerceIn(0f, 1f),
    blue = blue.coerceIn(0f, 1f),
    alpha = alpha.coerceIn(0f, 1f),
)
