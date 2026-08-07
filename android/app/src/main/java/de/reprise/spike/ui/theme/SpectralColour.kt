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
