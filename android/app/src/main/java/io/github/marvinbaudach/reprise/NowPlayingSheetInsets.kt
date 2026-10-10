package io.github.marvinbaudach.reprise

import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.asPaddingValues
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.material3.NavigationBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.Dp

/**
 * The sides of the screen the Now Playing sheet keeps its transport clear of
 * the navigation bar on.
 *
 * The root consumes only the status bar, so the sheet is the one place that
 * spends these insets. The stacked scene is a full-width column and spends both
 * sides; the wide-short sheet sits beside the library's rail, which already
 * spends the start inset, so only the end is left to it.
 */
internal val STACKED_SHEET_INSET_SIDES = WindowInsetsSides.Bottom + WindowInsetsSides.Horizontal
internal val WIDE_SHORT_SHEET_INSET_SIDES = WindowInsetsSides.Bottom + WindowInsetsSides.End

/** Keeps what follows out of the navigation bar on [sides]. */
@Composable
internal fun Modifier.sheetNavigationBarPadding(sides: WindowInsetsSides): Modifier =
    windowInsetsPadding(NavigationBarDefaults.windowInsets.only(sides))

/**
 * How far the sheet's transport row is lifted by [sheetNavigationBarPadding]
 * along the bottom edge, for a layout that has to know where the row ended up.
 */
@Composable
internal fun sheetNavigationBarBottomInset(): Dp =
    NavigationBarDefaults.windowInsets.only(WindowInsetsSides.Bottom).asPaddingValues().calculateBottomPadding()
