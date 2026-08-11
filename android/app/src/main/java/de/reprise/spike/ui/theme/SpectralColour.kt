package de.reprise.spike.ui.theme

import androidx.compose.ui.graphics.Color

/** The sole Compose conversion for RGB channels already selected by Rust. */
internal fun spectralColour(
    red: Double,
    green: Double,
    blue: Double,
    alpha: Float,
): Color = Color(
    red = red.coerceIn(0.0, 1.0).toFloat(),
    green = green.coerceIn(0.0, 1.0).toFloat(),
    blue = blue.coerceIn(0.0, 1.0).toFloat(),
    alpha = alpha,
)

/** The same theme-owned conversion for the flat visualizer's Float buffer. */
internal fun spectralColour(
    red: Float,
    green: Float,
    blue: Float,
    alpha: Float,
): Color = Color(
    red = red.coerceIn(0f, 1f),
    green = green.coerceIn(0f, 1f),
    blue = blue.coerceIn(0f, 1f),
    alpha = alpha,
)

/** Converts the Core's packed ARGB artwork colour at the theme boundary. */
internal fun Int.toComposeColor(): Color = Color(
    red = (this ushr 16 and 0xff) / 255f,
    green = (this ushr 8 and 0xff) / 255f,
    blue = (this and 0xff) / 255f,
    alpha = (this ushr 24 and 0xff) / 255f,
)
