package io.github.marvinbaudach.reprise

import androidx.activity.ComponentActivity
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshots.Snapshot
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.LifecycleRegistry
import androidx.lifecycle.compose.LocalLifecycleOwner
import io.github.marvinbaudach.reprise.scene.SpectrogramFrames
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/** How often, and how long, a surface without final analysis asks for the decoded part. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class SpectralSeekProgressPollingTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private val analysis = PollingAnalysis()
    private val trackId = mutableStateOf(9L)
    private val active = mutableStateOf(true)
    private var shown: PartialTrackAnalysis? = null
    private var compositions = 0

    @Test
    fun nav_15d_polls_about_once_per_interval_while_decoding() {
        analysis.answer = { decoded(0.3f) }
        show()
        val before = analysis.polls

        compose.mainClock.advanceTimeBy(INTERVALS * ANALYSIS_PROGRESS_POLL_MS)

        val polls = analysis.polls - before
        assertTrue("$polls polls in $INTERVALS intervals", polls in INTERVALS - 1..INTERVALS + 1)
    }

    @Test
    fun nav_15d_no_polls_once_the_final_data_arrived() {
        analysis.answer = { decoded(0.3f) }
        show()
        compose.mainClock.advanceTimeBy(ANALYSIS_PROGRESS_POLL_MS)

        Snapshot.withMutableSnapshot { active.value = false }
        settle()
        val before = analysis.polls
        compose.mainClock.advanceTimeBy(INTERVALS * ANALYSIS_PROGRESS_POLL_MS)

        assertEquals(before, analysis.polls)
        assertNull("final data replaces the partial at once", shown)
    }

    @Test
    fun nav_15d_no_polls_while_the_screen_is_not_started() {
        analysis.answer = { decoded(0.3f) }
        val lifecycle = TestLifecycle(Lifecycle.State.CREATED)
        show(lifecycle)
        compose.mainClock.advanceTimeBy(INTERVALS * ANALYSIS_PROGRESS_POLL_MS)
        assertEquals("a hidden screen polled", 0, analysis.polls)

        compose.runOnUiThread { lifecycle.registry.currentState = Lifecycle.State.STARTED }
        compose.mainClock.advanceTimeBy(INTERVALS * ANALYSIS_PROGRESS_POLL_MS)

        assertTrue("the visible screen never polled", analysis.polls > 0)
    }

    @Test
    fun nav_15d_a_track_that_never_reports_progress_stops_polling_until_the_next_revision() {
        show()
        compose.mainClock.advanceTimeBy((MAX_EMPTY_PROGRESS_POLLS + INTERVALS) * ANALYSIS_PROGRESS_POLL_MS)
        val stopped = analysis.polls
        assertTrue("$stopped polls for a track that never decodes", stopped <= MAX_EMPTY_PROGRESS_POLLS + 1)
        compose.mainClock.advanceTimeBy(INTERVALS * ANALYSIS_PROGRESS_POLL_MS)
        assertEquals(stopped, analysis.polls)

        analysis.bump()
        compose.mainClock.advanceTimeBy(INTERVALS * ANALYSIS_PROGRESS_POLL_MS)

        assertTrue("a new revision did not restart polling", analysis.polls > stopped)
    }

    @Test
    fun nav_15d_a_poll_waits_for_the_previous_answer() {
        analysis.holding = true
        show()

        compose.mainClock.advanceTimeBy(INTERVALS * ANALYSIS_PROGRESS_POLL_MS)

        assertEquals("polls piled up behind an unanswered read", 1, analysis.polls)
    }

    @Test
    fun nav_15d_a_late_answer_from_an_earlier_revision_is_not_shown() {
        analysis.answer = { decoded(0.3f) }
        show()
        assertNotNull(shown)
        analysis.holding = true
        compose.mainClock.advanceTimeBy(ANALYSIS_PROGRESS_POLL_MS)
        val late = analysis.held.single()

        // The decode ended without a result: the next revision's poll clears the partial.
        analysis.holding = false
        analysis.answer = { null }
        analysis.bump()
        settle()
        assertNull("the ended decode's partial stayed", shown)
        Snapshot.withMutableSnapshot { late(decoded(0.4f)) }
        settle()

        assertNull("an answer from the earlier revision brought the partial back", shown)
    }

    @Test
    fun nav_15d_a_late_answer_for_the_previous_track_is_not_shown() {
        analysis.answer = { decoded(0.3f) }
        analysis.holding = true
        show()
        val late = analysis.held.single()

        Snapshot.withMutableSnapshot { trackId.value = 10L }
        settle()
        Snapshot.withMutableSnapshot { late(decoded(0.4f)) }
        settle()

        assertNull(shown)
    }

    @Test
    fun nav_15d_an_unchanged_answer_does_not_recompose() {
        analysis.answer = { decoded(0.3f) }
        show()
        compose.mainClock.advanceTimeBy(ANALYSIS_PROGRESS_POLL_MS)
        val settled = compositions

        compose.mainClock.advanceTimeBy(INTERVALS * ANALYSIS_PROGRESS_POLL_MS)

        assertEquals("an identical picture recomposed the seek bar", settled, compositions)
    }

    private fun show(lifecycle: LifecycleOwner? = null) {
        compose.mainClock.autoAdvance = false
        compose.setContent {
            val owner = lifecycle ?: LocalLifecycleOwner.current
            CompositionLocalProvider(LocalLifecycleOwner provides owner) {
                val progress = rememberAnalysisProgress(
                    analysis,
                    trackId.value,
                    COUNT,
                    analysis.revision,
                    active.value,
                )
                SideEffect {
                    compositions += 1
                    shown = progress
                }
            }
        }
        settle()
    }

    /** A state write lands one frame after the effect that made it. */
    private fun settle() = repeat(SETTLE_FRAMES) { compose.mainClock.advanceTimeByFrame() }

    private companion object {
        const val COUNT = 64
        const val INTERVALS = 5L
        const val SETTLE_FRAMES = 3
    }
}

/** A new instance each time, as every real poll delivers. */
private fun decoded(fraction: Float) = PartialTrackAnalysis(
    coveredFraction = fraction,
    bars = List(10) { SpectralBar(false, 0.5f, 1.0, 0.0, 0.0) },
    frames = SpectrogramFrames(bandCount = 2, frameRateHz = 20, cells = ByteArray(8)),
)

private class TestLifecycle(initial: Lifecycle.State) : LifecycleOwner {
    val registry = LifecycleRegistry.createUnsafe(this).apply { currentState = initial }
    override val lifecycle: Lifecycle
        get() = registry
}

private class PollingAnalysis : TrackAnalysisPort {
    var answer: () -> PartialTrackAnalysis? = { null }
    var holding = false
    val held = mutableListOf<(PartialTrackAnalysis?) -> Unit>()
    var polls = 0
    override var revision by mutableLongStateOf(0L)
        private set

    fun bump() {
        Snapshot.withMutableSnapshot { revision += 1L }
    }

    override fun prepare(trackId: Long) = Unit

    override fun loadBars(trackId: Long, count: Int, deliver: (List<SpectralBar>?) -> Unit) =
        deliver(null)

    override fun loadProgress(
        trackId: Long,
        count: Int,
        deliver: (PartialTrackAnalysis?) -> Unit,
    ) {
        polls += 1
        if (holding) held += deliver else deliver(answer())
    }
}
