package io.github.marvinbaudach.reprise

import androidx.activity.ComponentActivity
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.click
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performTouchInput
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
import kotlin.math.roundToInt

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36], qualifiers = "w412dp-h916dp-port")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class SettingsOverlayTransitionTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    fun theOverlayArrivesFromTheRightOverThePageSlideDuration() {
        val visible = mutableStateOf(false)
        mountOverlay(visible = { visible.value })

        compose.mainClock.autoAdvance = false
        compose.runOnUiThread { visible.value = true }
        compose.waitForIdle()
        val left = captureMidTransitionLeft()
        val hostWidth = compose.activity.resources.displayMetrics.widthPixels

        assertTrue("the overlay is still arriving (left $left)", left in 1 until hostWidth)

        compose.mainClock.advanceTimeBy(SETTINGS_PAGE_SLIDE_MS / 2L)
        compose.mainClock.advanceTimeByFrame()
        assertEquals(0, overlayLeft())
    }

    @Test
    fun theOverlayLeavesToTheRightAndIsGoneAfterwards() {
        val visible = mutableStateOf(true)
        mountOverlay(visible = { visible.value })

        compose.mainClock.autoAdvance = false
        compose.runOnUiThread { visible.value = false }
        compose.waitForIdle()
        val left = captureMidTransitionLeft()
        val hostWidth = compose.activity.resources.displayMetrics.widthPixels

        assertTrue("the overlay is leaving to the right (left $left)", left in 1 until hostWidth)

        compose.mainClock.advanceTimeBy(SETTINGS_PAGE_SLIDE_MS / 2L)
        compose.mainClock.advanceTimeByFrame()
        compose.onAllNodesWithTag(OVERLAY_TAG).assertCountEquals(0)
    }

    @Test
    fun anOverlayRestoredOpenStandsStillFromTheFirstFrame() {
        compose.mainClock.autoAdvance = false
        mountOverlay(visible = { true })

        compose.mainClock.advanceTimeByFrame()

        compose.onAllNodesWithTag(OVERLAY_TAG).assertCountEquals(1)
        assertEquals(0, overlayLeft())
    }

    @Test
    fun aDepartingOverlayStopsItsPayloadReceivingTaps() {
        val visible = mutableStateOf(true)
        var clicks = 0
        mountOverlay(visible = { visible.value }, payloadClick = { clicks++ })

        compose.onNodeWithTag(PAYLOAD_TAG).performTouchInput { click() }
        assertEquals("the payload is interactive while open", 1, clicks)

        compose.mainClock.autoAdvance = false
        compose.runOnUiThread { visible.value = false }
        compose.waitForIdle()
        captureMidTransitionLeft()
        compose.onNodeWithTag(PAYLOAD_TAG).performTouchInput {
            click(Offset(1f, height / 2f))
        }

        assertEquals("the departing payload ignores taps", 1, clicks)
    }

    private fun captureMidTransitionLeft(): Int {
        compose.mainClock.advanceTimeByFrame()
        compose.mainClock.advanceTimeBy(SETTINGS_PAGE_SLIDE_MS / 2L)

        // The overlay is composed: the transition is in flight, not a hard cut.
        compose.onAllNodesWithTag(OVERLAY_TAG).assertCountEquals(1)
        return overlayLeft()
    }

    private fun overlayLeft(): Int = with(compose.density) {
        compose.onNodeWithTag(OVERLAY_TAG).getUnclippedBoundsInRoot().left.toPx().roundToInt()
    }

    private fun mountOverlay(visible: () -> Boolean, payloadClick: () -> Unit = {}) {
        val theme = MobileThemeSelection(
            palette = MobileTheme.NOCTURNE,
            colorScheme = AndroidColorScheme.SYSTEM,
            dynamicAvailable = false,
        )
        compose.setContent {
            RepriseTheme(theme, darkPalette = true) {
                SettingsOverlay(visible = visible()) {
                    Box(
                        Modifier
                            .fillMaxSize()
                            .clickable(onClick = payloadClick)
                            .testTag(PAYLOAD_TAG),
                    )
                }
            }
        }
    }

    private companion object {
        const val OVERLAY_TAG = "settings-overlay"
        const val PAYLOAD_TAG = "payload"
    }
}
