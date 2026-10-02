package io.github.marvinbaudach.reprise

import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.MutableState
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

internal val LocalLibraryStatusTopInset = staticCompositionLocalOf<MutableState<Dp>> {
    mutableStateOf(0.dp)
}

@Composable
internal fun Modifier.reportLibraryStatusTopInset(testTag: String): Modifier {
    val inset = LocalLibraryStatusTopInset.current
    val density = LocalDensity.current
    DisposableEffect(inset) {
        onDispose { inset.value = 0.dp }
    }
    return testTag(testTag).onSizeChanged { size ->
        inset.value = with(density) { size.height.toDp() }
    }
}
