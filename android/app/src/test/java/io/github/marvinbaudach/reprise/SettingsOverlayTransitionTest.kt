package io.github.marvinbaudach.reprise

import androidx.activity.ComponentActivity
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithTag
import io.github.marvinbaudach.reprise.settings.SETTINGS_PAGE_SLIDE_MS
import io.github.marvinbaudach.reprise.settings.SettingsOverlay
import io.github.marvinbaudach.reprise.ui.theme.RepriseTheme
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import uniffi.reprise_android_ffi.AndroidColorScheme

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36], qualifiers = "w412dp-h916dp-port")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class SettingsOverlayTransitionTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    fun theOverlayArrivesFromTheRightOverThePageSlideDuration() {
        val visible = mutableStateOf(false)
        mountOverlay { visible.value }

        compose.mainClock.autoAdvance = false
        compose.runOnUiThread { visible.value = true }
        compose.waitForIdle()
        val left = captureMidTransitionLeft()
        val hostWidth = compose.activity.resources.displayMetrics.widthPixels.toFloat()

        compose.onAllNodesWithTag(OVERLAY_TAG).assertCountEquals(1)
        assertTrue("the overlay is still arriving (left $left)", left > 0f && left < hostWidth)

        compose.mainClock.autoAdvance = true
        compose.waitForIdle()
        assertEquals(0f, overlayLeft())
    }

    @Test
    fun theOverlayLeavesToTheRightAndIsGoneAfterwards() {
        val visible = mutableStateOf(true)
        mountOverlay { visible.value }

        compose.mainClock.autoAdvance = false
        compose.runOnUiThread { visible.value = false }
        compose.waitForIdle()
        val left = captureMidTransitionLeft()

        compose.onAllNodesWithTag(OVERLAY_TAG).assertCountEquals(1)
        assertTrue("the overlay is leaving to the right (left $left)", left > 0f)

        compose.mainClock.autoAdvance = true
        compose.waitForIdle()
        compose.onAllNodesWithTag(OVERLAY_TAG).assertCountEquals(0)
    }

    @Test
    fun anOverlayRestoredOpenStandsStillFromTheFirstFrame() {
        compose.mainClock.autoAdvance = false
        mountOverlay { true }

        compose.mainClock.advanceTimeByFrame()

        compose.onAllNodesWithTag(OVERLAY_TAG).assertCountEquals(1)
        assertEquals(0f, overlayLeft())
    }

    private fun captureMidTransitionLeft(): Float {
        compose.mainClock.advanceTimeByFrame()
        compose.mainClock.advanceTimeBy(SETTINGS_PAGE_SLIDE_MS / 2L)
        return overlayLeft()
    }

    private fun overlayLeft(): Float = with(compose.density) {
        compose.onNodeWithTag(OVERLAY_TAG).getUnclippedBoundsInRoot().left.toPx()
    }

    private fun mountOverlay(visible: () -> Boolean) {
        val theme = MobileThemeSelection(
            palette = MobileTheme.NOCTURNE,
            colorScheme = AndroidColorScheme.SYSTEM,
            dynamicAvailable = false,
        )
        compose.setContent {
            RepriseTheme(theme, darkPalette = true) {
                SettingsOverlay(visible = visible()) {
                    Box(Modifier.fillMaxSize().testTag("payload"))
                }
            }
        }
    }

    private companion object {
        const val OVERLAY_TAG = "settings-overlay"
    }
}
