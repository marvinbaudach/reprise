package io.github.marvinbaudach.reprise.settings

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.tween
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag

/**
 * The opaque surface the settings graph is drawn inside, and the one edge the
 * graph does not own: entering and leaving the settings as a whole. It moves
 * exactly like a page of the graph — in from the right, out to the right, over
 * [SETTINGS_PAGE_SLIDE_MS] — so the overlay's edge and the pages inside it
 * read as one stack. Measured before this existed: both edges were one-frame
 * cuts next to 36-frame page slides.
 */
@Composable
internal fun SettingsOverlay(
    visible: Boolean,
    modifier: Modifier = Modifier,
    content: @Composable () -> Unit,
) {
    AnimatedVisibility(
        visible = visible,
        modifier = modifier,
        enter = slideInHorizontally(tween(SETTINGS_PAGE_SLIDE_MS)) { width -> width },
        exit = slideOutHorizontally(tween(SETTINGS_PAGE_SLIDE_MS)) { width -> width },
    ) {
        Surface(
            modifier = Modifier.fillMaxSize().testTag("settings-overlay"),
            color = MaterialTheme.colorScheme.background,
            content = content,
        )
    }
}
