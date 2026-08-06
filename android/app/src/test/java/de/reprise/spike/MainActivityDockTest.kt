package de.reprise.spike

import android.os.Looper
import android.view.View
import android.view.WindowManager
import androidx.compose.ui.test.assertHeightIsEqualTo
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertTextContains
import androidx.compose.ui.test.assertWidthIsEqualTo
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(
    sdk = [36],
    qualifiers = "w412dp-h916dp-port",
    application = ConfigurationTestApplication::class,
)
@Suppress("DEPRECATION") // Robolectric exposes immersive state only through the legacy mirror.
class MainActivityDockTest {
    @get:Rule
    val compose = createAndroidComposeRule<MainActivity>()

    private val application: ConfigurationTestApplication
        get() = RuntimeEnvironment.getApplication() as ConfigurationTestApplication

    @After
    fun releaseTheService() {
        application.releaseService()
    }

    @Test
    fun rotatingOnlyOffersDockAndItsHeartWritesOnlyZeroOrFiveAcrossTracks() {
        application.trackRatings[2] = 5
        publishTrack(1)
        recreateAt("w916dp-h412dp-land")

        compose.onNodeWithText("Dock mode").assertIsDisplayed()
        compose.onNodeWithTag("dock-surface").assertDoesNotExist()
        assertFalse(keepScreenOn())

        compose.onNodeWithText("Dock mode").performClick()
        compose.onNodeWithTag("dock-surface").assertIsDisplayed()
        compose.onNodeWithTag("dock-cover").assertWidthIsEqualTo(290.dp)
        compose.onNodeWithTag("dock-play").assertWidthIsEqualTo(96.dp)
        compose.onNodeWithTag("dock-previous").assertWidthIsEqualTo(76.dp)
        compose.onNodeWithTag("dock-next").assertWidthIsEqualTo(76.dp)
        compose.onNodeWithTag("dock-heart").assertWidthIsEqualTo(64.dp)
        compose.onNodeWithTag("dock-heart").assertHeightIsEqualTo(64.dp)
        compose.onNodeWithTag("dock-title").assertTextContains("Rotation Song 1")
        compose.onNodeWithTag("dock-clock").assertIsDisplayed()
        compose.onNodeWithTag("library-navigation-rail").assertDoesNotExist()
        compose.onNodeWithContentDescription("Add to favourites").assertIsDisplayed()
        compose.onNodeWithText("0 plays").assertDoesNotExist()

        compose.onNodeWithTag("dock-heart").performClick()
        compose.onNodeWithTag("dock-heart").performClick()
        compose.onNodeWithTag("dock-heart").performClick()
        assertEquals(
            listOf(1L to 5, 1L to 0, 1L to 5),
            application.controls.ratingRequests,
        )

        publishTrack(2)
        compose.onNodeWithTag("dock-title").assertTextContains("Rotation Song 2")
        compose.onNodeWithTag("dock-heart").performClick()
        assertEquals(listOf(1L to 5, 1L to 0, 1L to 5, 2L to 0), application.controls.ratingRequests)

        recreateAt("w412dp-h916dp-port")
        compose.onNodeWithTag("dock-surface").assertDoesNotExist()
        assertFalse(keepScreenOn())
    }

    @Test
    fun dockRequestsImmersiveAndKeepScreenOnlyUntilTheExplicitExit() {
        enterDock()

        assertTrue(keepScreenOn())
        assertTrue(
            compose.activity.window.decorView.systemUiVisibility and
                View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY != 0,
        )

        compose.onNodeWithContentDescription("Exit dock mode").performClick()
        compose.waitForIdle()

        assertFalse(keepScreenOn())
        assertEquals(
            0,
            compose.activity.window.decorView.systemUiVisibility and
                View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY,
        )
    }

    @Test
    fun destroyingTheDockedActivityClearsKeepScreenWithoutAnExitAction() {
        enterDock()
        val oldWindow = compose.activity.window
        assertTrue(shadowOf(oldWindow).getFlag(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON))

        compose.activityRule.scenario.moveToState(Lifecycle.State.DESTROYED)
        shadowOf(Looper.getMainLooper()).idle()

        assertFalse(shadowOf(oldWindow).getFlag(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON))
    }

    private fun enterDock() {
        publishTrack(1)
        recreateAt("w916dp-h412dp-land")
        compose.onNodeWithText("Dock mode").performClick()
        compose.waitForIdle()
    }

    private fun publishTrack(trackId: Long) {
        application.service.publish(m9bSnapshot(trackId))
        shadowOf(Looper.getMainLooper()).idle()
        compose.waitForIdle()
    }

    private fun recreateAt(qualifiers: String) {
        RuntimeEnvironment.setQualifiers(qualifiers)
        compose.activityRule.scenario.recreate()
        shadowOf(Looper.getMainLooper()).idle()
        application.service.republish()
        shadowOf(Looper.getMainLooper()).idle()
        compose.waitForIdle()
    }

    private fun keepScreenOn(): Boolean = shadowOf(compose.activity.window)
        .getFlag(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
}
