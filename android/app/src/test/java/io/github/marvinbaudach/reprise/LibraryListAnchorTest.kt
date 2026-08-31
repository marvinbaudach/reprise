package io.github.marvinbaudach.reprise

import androidx.activity.ComponentActivity
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.unit.dp
import kotlin.math.roundToInt
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * What a library list's anchor means, measured against a real laid-out list.
 *
 * These are not configuration-change claims — those belong to
 * [MainActivityConfigurationTest], which goes through the path a configuration
 * change really takes. What is measured here is the arithmetic that path
 * depends on: an offset recorded in one row height and read back in another.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36], qualifiers = "w412dp-h916dp-port")
class LibraryListAnchorTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    fun aRememberedFractionOpensTheListAtThatFractionOfItsOwnRow() {
        lateinit var state: LazyListState
        var rowPx = 0
        compose.setContent {
            rowPx = with(LocalDensity.current) { WIDE_SHORT_ROW_DP.dp.roundToPx() }
            state = rememberLibraryListState(
                LibraryScrollPosition(firstVisibleItemIndex = 5, itemOffsetFraction = 0.5f),
            )
            RowsOf(WIDE_SHORT_ROW_DP, state)
        }
        compose.waitForIdle()

        assertEquals(5, state.firstVisibleItemIndex)
        assertEquals((rowPx * 0.5f).roundToInt(), state.firstVisibleItemScrollOffset)
    }

    @Test
    fun aMidRowScrollIsRememberedAsAFractionOfTheRowItStartsIn() {
        val surfaceState = MobileSurfaceViewModel()
        lateinit var state: LazyListState
        var rowPx = 0
        compose.setContent {
            rowPx = with(LocalDensity.current) { STACKED_ROW_DP.dp.roundToPx() }
            state = rememberLibraryListState(LibraryScrollPosition())
            ObserveLibraryListAnchor(LibraryListKey.TITLES, state, surfaceState)
            RowsOf(STACKED_ROW_DP, state)
        }
        compose.waitForIdle()
        compose.runOnIdle { runBlocking { state.scrollToItem(5, rowPx / 4) } }
        compose.waitForIdle()

        val anchor = surfaceState.scrollPosition(LibraryListKey.TITLES)
        assertEquals(5, anchor.firstVisibleItemIndex)
        assertEquals(0.25f, anchor.itemOffsetFraction, 0.01f)
        // A quarter of the way into a stacked row is a quarter of the way into
        // the shorter wide-short one, not the same number of pixels down it.
        val wideShortRowPx = with(compose.density) { WIDE_SHORT_ROW_DP.dp.roundToPx() }
        assertEquals((wideShortRowPx * 0.25f).roundToInt(), anchor.offsetPxIn(wideShortRowPx))
    }
}

private const val STACKED_ROW_DP = 72
private const val WIDE_SHORT_ROW_DP = 64

@Composable
private fun RowsOf(rowHeightDp: Int, state: LazyListState) {
    LazyColumn(state = state, modifier = Modifier.fillMaxSize()) {
        items(count = 40) { index ->
            Box(
                modifier = Modifier
                    .fillMaxWidth()
                    .height(rowHeightDp.dp),
            ) {
                Text("Row $index")
            }
        }
    }
}
