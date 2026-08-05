package de.reprise.spike

import androidx.activity.ComponentActivity
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.unit.DpOffset
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * Whether the ambient surface *moves*, asked of the surface rather than of the
 * flag.
 *
 * The activity-level tests read `ambientScheduleEvents`, which is the boolean
 * [AmbientMotionController.update] computes and reports in the same call — a
 * value confirming it is itself. Today the drawing branch happens to be chosen
 * from that same boolean, so those tests pass for a reason no test states, and
 * a regression that left the flag correct while drawing the wrong branch would
 * go through them untouched.
 *
 * These two stop the clock, look at where a field is drawn, move the clock, and
 * look again. The three fields carry the same marks in both branches, so
 * neither test can tell them apart by anything except motion.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36], qualifiers = "w412dp-h916dp-port")
class AmbientMotionTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    fun aScheduledAmbientSurfaceMovesTheFieldItDraws() {
        val controller = ambientSurface()

        compose.runOnUiThread {
            controller.runtimeChanged(
                resumed = true,
                screenInteractive = true,
                animationsEnabled = true,
            )
        }
        compose.mainClock.advanceTimeBy(FRAME_MS)

        val start = fieldOrigin()
        compose.mainClock.advanceTimeBy(DRIFT_SAMPLE_MS)

        assertNotEquals(start, fieldOrigin())
    }

    /**
     * The other half, and the one that would catch a surface drifting behind a
     * flag that says it is not: with the system's animations off, the same
     * elapsed time may not move anything.
     */
    @Test
    fun anUnscheduledAmbientSurfaceHoldsTheFieldStill() {
        val controller = ambientSurface()

        compose.runOnUiThread {
            controller.runtimeChanged(
                resumed = true,
                screenInteractive = true,
                animationsEnabled = false,
            )
        }
        compose.mainClock.advanceTimeBy(FRAME_MS)

        val start = fieldOrigin()
        compose.mainClock.advanceTimeBy(DRIFT_SAMPLE_MS)

        assertEquals(start, fieldOrigin())
    }

    /** Composes the surface with a real controller and a clock that only moves on demand. */
    private fun ambientSurface(): AmbientMotionController {
        val controller = AmbientMotionController()
        compose.mainClock.autoAdvance = false
        compose.setContent {
            CompositionLocalProvider(LocalAmbientMotionController provides controller) {
                AmbientFields(artworkColors = null)
            }
        }
        compose.mainClock.advanceTimeBy(FRAME_MS)
        return controller
    }

    /** Where the first field is actually drawn, layer transform and all. */
    private fun fieldOrigin(): DpOffset {
        val bounds = compose.onNodeWithTag("ambient-field-0").getUnclippedBoundsInRoot()
        return DpOffset(bounds.left, bounds.top)
    }
}

private const val FRAME_MS = 16L

/** Long enough that the shortest field period has moved well past measuring noise. */
private const val DRIFT_SAMPLE_MS = 3_000L
