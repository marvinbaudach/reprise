package io.github.marvinbaudach.reprise.ui.theme

import androidx.compose.ui.graphics.Color
import androidx.glance.color.ColorProvider

/**
 * The home-screen widget's colours. A widget is drawn by the launcher and cannot
 * read the app's Compose theme, so it carries day and night values of its own:
 * the night ones are the Nocturne surface and text colours.
 */
internal val WidgetBackground = ColorProvider(day = Color(0xFFF2F4F6), night = Color(0xFF161826))
internal val WidgetPrimaryText = ColorProvider(day = Color(0xFF14171C), night = Color(0xFFE9E9ED))
internal val WidgetSecondaryText = ColorProvider(day = Color(0xFF5A6270), night = Color(0xFFB2B6CA))
internal val WidgetControlTint = ColorProvider(day = Color(0xFF14171C), night = Color(0xFFE9E9ED))
internal val WidgetScrim = ColorProvider(day = Color(0x66000000), night = Color(0x66000000))
internal val WidgetOnScrim = ColorProvider(day = Color(0xFFFFFFFF), night = Color(0xFFFFFFFF))
