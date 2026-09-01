package io.github.marvinbaudach.reprise

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Assert.assertNull
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
        assertEquals(-569.2f, nowPlayingTitleTranslation(positionPx = 400f, widthPx = 400f), 0.001f)

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
