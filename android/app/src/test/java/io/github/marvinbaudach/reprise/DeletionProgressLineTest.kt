package io.github.marvinbaudach.reprise

import androidx.activity.ComponentActivity
import androidx.compose.material3.MaterialTheme
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithText
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * The screen's line for a running deletion is bounded: a provider that never
 * answers must not leave a promise of progress up forever, and must not be
 * accused of failing either.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36], qualifiers = "w412dp-h916dp-port")
class DeletionProgressLineTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private val surface = MobileSurfaceViewModel()

    private fun show() = compose.setContent {
        MaterialTheme { DeletionMessageLine(surface) }
    }

    @Test
    fun aDeletionThatOutlastsTheBoundStopsPromisingProgressWithoutClaimingFailure() {
        show()
        surface.begin("Deleting 40 tracks…")
        compose.waitForIdle()
        compose.onNodeWithText("Deleting 40 tracks…").assertIsDisplayed()

        compose.mainClock.advanceTimeBy(DELETION_STILL_RUNNING_MS + 1)
        compose.waitForIdle()

        compose.onNodeWithText("Deleting 40 tracks…").assertDoesNotExist()
        compose.onNodeWithText(DELETION_STILL_RUNNING_TEXT).assertIsDisplayed()
    }

    @Test
    fun aLateAnswerStillReplacesTheStillDeletingLine() {
        show()
        val run = surface.begin("Deleting 40 tracks…")
        compose.mainClock.advanceTimeBy(DELETION_STILL_RUNNING_MS + 1)
        compose.waitForIdle()

        run.finish("40 tracks deleted")
        compose.waitForIdle()

        compose.onNodeWithText(DELETION_STILL_RUNNING_TEXT).assertDoesNotExist()
        compose.onNodeWithText("40 tracks deleted").assertIsDisplayed()
    }

    @Test
    fun aDeletionStartedLaterGetsItsOwnFullBound() {
        show()
        surface.begin("Deleting 40 tracks…")
        compose.mainClock.advanceTimeBy(DELETION_STILL_RUNNING_MS - 1_000)
        surface.begin("Deleting 2 tracks…")
        compose.mainClock.advanceTimeBy(2_000)
        compose.waitForIdle()

        compose.onNodeWithText("Deleting 2 tracks…").assertIsDisplayed()
    }
}
