package io.github.marvinbaudach.reprise.ui.theme

import android.content.Context
import android.os.Build
import androidx.compose.material3.ColorScheme
import androidx.compose.material3.dynamicDarkColorScheme
import androidx.compose.material3.dynamicLightColorScheme
import androidx.compose.runtime.Composable
import io.github.marvinbaudach.reprise.MobileTheme
import io.github.marvinbaudach.reprise.MobileThemeSelection

/** Chooses the wallpaper-seeded Material palette behind the API 31 guard. */
@Composable
internal fun androidColorScheme(
    context: Context,
    selection: MobileThemeSelection,
    darkPalette: Boolean,
): ColorScheme {
    if (selection.palette != MobileTheme.DYNAMIC ||
        !selection.dynamicAvailable ||
        Build.VERSION.SDK_INT < Build.VERSION_CODES.S
    ) {
        return nocturneColorScheme()
    }
    return if (darkPalette) {
        dynamicDarkColorScheme(context)
    } else {
        dynamicLightColorScheme(context)
    }
}
