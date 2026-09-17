package io.github.marvinbaudach.reprise

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertSame
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class NowPlayingPanelsTest {
    @Test
    fun symmetric_window_maps_the_current_track_and_both_neighbours_to_absolute_indices() {
        val previous = panelTrack(10)
        val current = panelTrack(11)
        val next = panelTrack(12)

        val window = playPanelWindow(
            currentIndex = 7,
            currentTrackId = current.id,
            rows = listOf(previous, current, next),
        )

        assertEquals(listOf(6, 7, 8), window.panels.map { it.index })
        assertEquals(listOf(10L, 11L, 12L), window.panels.map { it.track.id })
        assertEquals(6, window.firstIndex)
        assertEquals(8, window.lastIndex)
    }

    @Test
    fun clipped_first_window_still_places_the_current_track_at_zero() {
        val current = panelTrack(20)
        val next = panelTrack(21)

        val window = playPanelWindow(0, current.id, listOf(current, next))

        assertEquals(listOf(0, 1), window.panels.map { it.index })
        assertEquals(0, window.firstIndex)
        assertEquals(1, window.lastIndex)
    }

    @Test
    fun the_first_queue_position_has_no_panel_before_it() {
        val current = panelTrack(20)
        val next = panelTrack(21)

        val window = playPanelWindow(0, current.id, listOf(current, next))

        assertEquals(0, window.firstIndex)
        assertFalse(window.panels.any { panel -> panel.index == -1 })
    }

    @Test
    fun the_last_queue_position_has_no_panel_after_it() {
        val previous = panelTrack(20)
        val current = panelTrack(21)

        val window = playPanelWindow(4, current.id, listOf(previous, current))

        assertEquals(4, window.lastIndex)
        assertFalse(window.panels.any { panel -> panel.index == 5 })
    }

    @Test
    fun an_unknown_window_claims_no_neighbour_in_either_direction() {
        val window = placeholderPlayPanelWindow(panelTrack(20), currentIndex = 7)

        assertEquals(7, window.firstIndex)
        assertEquals(7, window.lastIndex)
    }

    @Test
    fun a_known_index_change_replaces_the_centre_without_discarding_its_neighbour() {
        val previous = panelTrack(20)
        val current = panelTrack(21)
        val next = panelTrack(22)
        val window = playPanelWindow(4, current.id, listOf(previous, current, next))

        val advanced = window.withCurrentPanel(next.copy(title = "Answered track"), currentIndex = 5)

        assertEquals(listOf(21L, 22L), advanced.panels.map { panel -> panel.track.id })
        assertEquals("Answered track", advanced.panels.last().track.title)
        assertEquals(3, advanced.firstIndex)
        assertEquals(5, advanced.lastIndex)
    }

    @Test
    fun an_index_that_arrives_before_its_track_keeps_the_prefetched_centre() {
        val previous = panelTrack(20)
        val current = panelTrack(21)
        val next = panelTrack(22)
        val window = playPanelWindow(4, current.id, listOf(previous, current, next))

        // The transport has moved to index 5 (track 22) but the sheet's answered
        // track still reads 21: the row for 22 is being read off the main thread.
        val advanced = window.withCurrentPanel(current, currentIndex = 5, currentTrackId = next.id)

        assertEquals(listOf(4, 5), advanced.panels.map { panel -> panel.index })
        assertEquals(listOf(21L, 22L), advanced.panels.map { panel -> panel.track.id })
        assertEquals(3, advanced.firstIndex)
        assertEquals(5, advanced.lastIndex)
    }

    @Test
    fun a_stale_row_fills_only_a_centre_nothing_was_prefetched_for() {
        val previous = panelTrack(20)
        val current = panelTrack(21)
        val window = playPanelWindow(4, current.id, listOf(previous, current))

        // Index 5 is outside the known window and no neighbour was prefetched:
        // the play view keeps its last answered row until the new one arrives.
        val jumped = window.withCurrentPanel(current, currentIndex = 5, currentTrackId = 30L)
        assertEquals(listOf(5), jumped.panels.map { panel -> panel.index })
        assertEquals(listOf(21L), jumped.panels.map { panel -> panel.track.id })

        val answered = jumped.withCurrentPanel(panelTrack(30), currentIndex = 5, currentTrackId = 30L)
        assertEquals(listOf(5), answered.panels.map { panel -> panel.index })
        assertEquals(listOf(30L), answered.panels.map { panel -> panel.track.id })
    }

    @Test
    fun two_transport_moves_before_the_row_answers_keep_what_was_prefetched() {
        val rows = (20L..25L).map(::panelTrack)
        val window = playPanelWindow(4, 22L, rows)
        assertEquals(listOf(3, 4, 5), window.panels.map { panel -> panel.index })

        val once = window.withCurrentPanel(panelTrack(22), currentIndex = 5, currentTrackId = 23L)
        assertEquals(listOf(4 to 22L, 5 to 23L), once.panels.map { it.index to it.track.id })

        // The second move outruns the prefetch: the neighbour stays, the
        // stale row holds the centre until the reload answers.
        val twice = once.withCurrentPanel(panelTrack(22), currentIndex = 6, currentTrackId = 24L)
        assertEquals(listOf(5 to 23L, 6 to 22L), twice.panels.map { it.index to it.track.id })
        assertEquals(2, twice.firstIndex)
        assertEquals(7, twice.lastIndex)
    }

    @Test
    fun the_prefetch_window_keeps_two_warm_but_only_renders_one_neighbour_per_side() {
        val rows = (30L..34L).map(::panelTrack)

        val window = playPanelWindow(currentIndex = 8, currentTrackId = 32, rows = rows)

        assertEquals(listOf(7, 8, 9), window.panels.map { it.index })
        assertEquals(6, window.firstIndex)
        assertEquals(10, window.lastIndex)
    }

    @Test
    fun panel_rest_state_is_bit_exact_and_neighbour_values_follow_the_design() {
        val rest = nowPlayingPanelTransform(panelIndex = 3, positionPx = 1_200f, widthPx = 400f)
        val next = nowPlayingPanelTransform(panelIndex = 4, positionPx = 1_200f, widthPx = 400f)

        assertEquals(0f.toRawBits(), rest.translationX.toRawBits())
        assertEquals(1f.toRawBits(), rest.scale.toRawBits())
        assertEquals(0f.toRawBits(), rest.rotationDegrees.toRawBits())
        assertEquals(1f.toRawBits(), rest.opacity.toRawBits())
        assertEquals(0f.toRawBits(), rest.blurPx.toRawBits())
        assertEquals(1f.toRawBits(), rest.saturation.toRawBits())
        assertNull(rest.rotationForLayer)

        assertEquals(400f, next.translationX, 0f)
        assertEquals(0.87f, next.scale, 0f)
        assertEquals(-3.5f, next.rotationDegrees, 0f)
        assertEquals(0.25f, next.opacity, 0f)
        assertEquals(5f, next.blurPx, 0f)
        assertEquals(0.4f, next.saturation, 0f)
    }

    @Test
    fun panel_and_glow_rest_state_stays_bit_exact_at_a_non_round_screen_width() {
        val widthPx = 342.33331f
        val positionPx = 3 * widthPx

        val panel = nowPlayingPanelTransform(panelIndex = 3, positionPx, widthPx)
        val glow = nowPlayingGlowTransform(panelIndex = 3, positionPx, widthPx)

        assertEquals(0f.toRawBits(), panel.translationX.toRawBits())
        assertEquals(0f.toRawBits(), panel.rotationDegrees.toRawBits())
        assertEquals(1f.toRawBits(), panel.opacity.toRawBits())
        assertNull(panel.rotationForLayer)
        assertEquals(0f.toRawBits(), glow.translationX.toRawBits())
        assertEquals(1f.toRawBits(), glow.opacity.toRawBits())
    }

    @Test
    fun title_runs_at_its_wider_ratio_while_progress_only_fades_and_compresses() {
        assertEquals(-512.8f, nowPlayingTitleTranslation(positionPx = 400f), 0.001f)
        // The resting panel carries no offset at all: any constant here shifts
        // every title off the screen centre for as long as nothing is dragged.
        assertEquals(0f, nowPlayingTitleTranslation(positionPx = 0f), 0f)
        // ...and the two neighbours have to sit the same distance out on either
        // side, or a swipe left and a swipe right do not mirror each other.
        assertEquals(
            -nowPlayingTitleTranslation(positionPx = -400f),
            nowPlayingTitleTranslation(positionPx = 400f),
            0.001f,
        )

        val progress = nowPlayingProgressTransform(currentIndex = 1, positionPx = 600f, widthPx = 400f)
        assertEquals(-35f, progress.translationY, 0f)
        assertEquals(0.55f, progress.opacity, 0f)
        assertEquals(0.97f, progress.scaleX, 0f)
    }

    @Test
    fun each_track_glow_uses_the_spatial_factor_and_distance_fade() {
        val current = nowPlayingGlowTransform(panelIndex = 2, positionPx = 800f, widthPx = 400f)
        val next = nowPlayingGlowTransform(panelIndex = 3, positionPx = 800f, widthPx = 400f)

        assertEquals(0f, current.translationX, 0f)
        assertEquals(1f, current.opacity, 0f)
        assertEquals(92f, next.translationX, 0f)
        assertEquals(0f, next.opacity, 0f)
    }

    @Test
    fun a_panel_with_ready_data_shows_its_bars_no_matter_where_it_sits() {
        // nowPlayingVisualBlend takes no position/near argument at all, so a neighbour sitting far
        // from the pager's centre — the reported symptom — cannot collapse its bars by distance the
        // way the old `near.pow(...)` formula did.
        val blend = nowPlayingVisualBlend(visualizerOpacity = 1f, dataAvailability = 1f)

        assertTrue("a panel with ready data must show its bars", blend.barsOpacity > 0f)
        assertEquals(1f, blend.barsOpacity, 0f)
        assertEquals(0f, blend.coverOpacity, 0f)
    }

    @Test
    fun a_panel_without_ready_data_falls_back_to_its_cover() {
        val blend = nowPlayingVisualBlend(visualizerOpacity = 1f, dataAvailability = 0f)

        assertEquals(0f, blend.barsOpacity, 0f)
        assertEquals(1f, blend.coverOpacity, 0f)
    }

    @Test
    fun choosing_cover_mode_still_wins_even_with_data_ready() {
        val blend = nowPlayingVisualBlend(visualizerOpacity = 0f, dataAvailability = 1f)

        assertEquals(0f, blend.barsOpacity, 0f)
        assertEquals(1f, blend.coverOpacity, 0f)
    }

    @Test
    fun a_live_panel_with_no_stored_spectrogram_and_no_captured_scene_yet_has_no_visual_data() {
        // Regression: this used to be `frames.frameCount > 0 || panel.index == currentIndex`, which
        // opened the bars slot the instant a panel became live, whether or not its engine had
        // anything to show yet. A track the desktop never analysed must keep its cover until its
        // engine has actually captured a real frame.
        assertFalse(
            "a live panel must not claim visual data before its engine captured a real frame",
            panelHasVisualData(
                storedFrameCount = 0,
                hasCapturedLiveScene = false,
                canMirrorLiveScene = false,
            ),
        )
    }

    @Test
    fun a_live_panel_opens_its_bars_once_its_engine_captured_a_real_frame() {
        assertTrue(
            panelHasVisualData(
                storedFrameCount = 0,
                hasCapturedLiveScene = true,
                canMirrorLiveScene = false,
            ),
        )
    }

    @Test
    fun a_stored_spectrogram_grants_visual_data_even_off_the_live_slot() {
        assertTrue(
            panelHasVisualData(
                storedFrameCount = 5,
                hasCapturedLiveScene = false,
                canMirrorLiveScene = false,
            ),
        )
    }

    @Test
    fun a_captured_scene_counts_off_the_live_slot_too() {
        // The outgoing panel keeps the bars it drew while live, and a neighbour
        // that mirrored the live scene during the swipe keeps those -- in
        // visualizer mode no panel falls back to its cover for want of data.
        assertTrue(
            panelHasVisualData(
                storedFrameCount = 0,
                hasCapturedLiveScene = true,
                canMirrorLiveScene = false,
            ),
        )
    }

    @Test
    fun a_mirroring_neighbour_has_visual_data_before_its_own_spectrogram_loads() {
        // Regression: the stored spectrogram loads async (`rememberSpectrogram`,
        // cache miss posts back later); until it lands, `storedFrameCount == 0`
        // and `hasCapturedLiveScene == false` for a neighbour that has never
        // been live. It is mirroring the live panel's engine right now, so it
        // must count as having visual data instead of flashing its cover up
        // for the frames before the cache answers (measured ~80 ms on device).
        assertTrue(
            "a mirroring neighbour must not fall back to its cover while data loads",
            panelHasVisualData(
                storedFrameCount = 0,
                hasCapturedLiveScene = false,
                canMirrorLiveScene = true,
            ),
        )
        val blend = nowPlayingVisualBlend(visualizerOpacity = 1f, dataAvailability = 1f)
        assertEquals(0f, blend.coverOpacity, 0f)
    }

    @Test
    fun a_resting_neighbour_eligible_to_mirror_already_counts_as_pictured() {
        // Regression: `panelHasVisualData`'s third argument used to be
        // `near > 0f && liveSceneAvailable` (`isMirroringLiveScene`), so
        // `dataAvailability` rested at 0 for a spectrogram-less neighbour until
        // `near` turned positive on the first drag pixel -- the 220ms crossfade
        // then flashed the cover up at the start of every swipe. Eligibility to
        // mirror must not depend on `near`: that gate belongs to
        // `panelMirrorsLiveScene` alone, which decides only whether the mirror
        // is actually drawn (a render-cost question), not whether data is
        // available.
        assertTrue(
            "a neighbour that would mirror the live scene once dragged onscreen already has data at rest",
            panelHasVisualData(
                storedFrameCount = 0,
                hasCapturedLiveScene = false,
                canMirrorLiveScene = true,
            ),
        )
    }

    @Test
    fun a_non_mirroring_panel_still_has_no_visual_data_with_nothing_captured() {
        assertFalse(
            panelHasVisualData(
                storedFrameCount = 0,
                hasCapturedLiveScene = false,
                canMirrorLiveScene = false,
            ),
        )
    }

    @Test
    fun a_visible_neighbour_mirrors_the_live_scene_and_a_resting_one_does_not() {
        assertTrue(panelMirrorsLiveScene(isLivePanel = false, storedFrameCount = 0, near = 0.4f, liveSceneAvailable = true))
        assertFalse("at rest the neighbour is off the screen", panelMirrorsLiveScene(isLivePanel = false, storedFrameCount = 0, near = 0f, liveSceneAvailable = true))
        assertFalse("a stored spectrogram is the panel's own scene", panelMirrorsLiveScene(isLivePanel = false, storedFrameCount = 3, near = 0.4f, liveSceneAvailable = true))
        assertFalse("the live panel is the source, not a mirror", panelMirrorsLiveScene(isLivePanel = true, storedFrameCount = 0, near = 1f, liveSceneAvailable = true))
        assertFalse("nothing to mirror before the live engine exists", panelMirrorsLiveScene(isLivePanel = false, storedFrameCount = 0, near = 0.4f, liveSceneAvailable = false))
    }

    @Test
    fun the_live_panel_polls_for_its_first_scene_only_while_bars_were_asked_for() {
        assertTrue(
            "the live panel must keep polling until it has ever captured a scene",
            panelAwaitsFirstLiveScene(
                visualizerOpacity = 1f,
                drawsLiveScene = true,
                hasCapturedLiveScene = false,
            ),
        )
        assertFalse(
            "cover mode must never pay for a scene it will not draw",
            panelAwaitsFirstLiveScene(
                visualizerOpacity = 0f,
                drawsLiveScene = true,
                hasCapturedLiveScene = false,
            ),
        )
        assertFalse(
            "a panel that neither owns nor mirrors the live scene never polls for it",
            panelAwaitsFirstLiveScene(
                visualizerOpacity = 1f,
                drawsLiveScene = false,
                hasCapturedLiveScene = false,
            ),
        )
        assertFalse(
            "once a real frame landed, the live panel stops polling",
            panelAwaitsFirstLiveScene(
                visualizerOpacity = 1f,
                drawsLiveScene = true,
                hasCapturedLiveScene = true,
            ),
        )
    }

    @Test
    fun only_the_current_panel_uses_the_live_audio_scene_factory() {
        val liveFactory = VisualSceneEngineFactory { error("not created by this unit test") }

        assertSame(liveFactory, visualSceneFactoryForPanel(live = true, liveFactory))
        assertSame(
            NativeVisualSceneEngineFactory,
            visualSceneFactoryForPanel(live = false, liveFactory),
        )
    }

    @Test
    fun every_per_frame_panel_canvas_captures_the_scene_revision() {
        val source = File("src/main/java/io/github/marvinbaudach/reprise/NowPlayingScene.kt").readText()
        val observation = "observeSceneFrame(drawRevision)"

        assertEquals(3, source.split(observation).size - 1)
    }

    private fun panelTrack(id: Long) = LibraryTrack(
        id = id,
        uri = "content://track/$id",
        title = "Track $id",
        artist = "Artist",
        album = "Album",
        durationMs = 120_000,
        playCount = 0,
        rating = 0,
    )
}
