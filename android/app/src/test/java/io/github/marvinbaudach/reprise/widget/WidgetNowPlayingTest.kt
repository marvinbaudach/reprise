package io.github.marvinbaudach.reprise.widget

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.reprise_android_ffi.AndroidPlaybackState

class WidgetNowPlayingTest {
    private val artwork = { uri: String -> "/cache/${uri.substringAfterLast('/')}.png" }

    private fun map(
        snapshot: uniffi.reprise_android_ffi.AndroidPlaybackSnapshot?,
        previous: WidgetNowPlaying = WidgetNowPlaying.Empty,
    ) = widgetNowPlaying(snapshot, previous, ::metadataFor, artwork)

    @Test
    fun aPlayingTrackShowsItsTitleArtistAndCover() {
        val state = map(snapshot(AndroidPlaybackState.PLAYING, 5))

        assertEquals(5L, state.trackId)
        assertEquals("Title 5", state.title)
        assertEquals("Artist 5", state.artist)
        assertEquals("/cache/5.flac.png", state.artworkPath)
        assertTrue(state.isPlaying)
        assertFalse(state.isEmpty)
    }

    @Test
    fun aPausedTrackIsNotPlaying() {
        assertFalse(map(snapshot(AndroidPlaybackState.PAUSED, 5)).isPlaying)
    }

    @Test
    fun buffering_and_stopped_do_not_count_as_playing() {
        assertFalse(map(snapshot(AndroidPlaybackState.BUFFERING, 5)).isPlaying)
        assertFalse(map(snapshot(AndroidPlaybackState.STOPPED, 5)).isPlaying)
    }

    @Test
    fun nothingPlayedYetIsTheEmptyState() {
        assertTrue(map(null).isEmpty)
        assertTrue(map(snapshot(AndroidPlaybackState.STOPPED, null)).isEmpty)
        assertEquals(WidgetNowPlaying.Empty, map(null))
    }

    @Test
    fun whenPlaybackRunsOutTheLastTrackStaysShownAndPaused() {
        val last = map(snapshot(AndroidPlaybackState.PLAYING, 5))

        val after = map(snapshot(AndroidPlaybackState.STOPPED, null), previous = last)

        assertEquals(last.copy(isPlaying = false), after)
    }

    @Test
    fun aPlayPauseFlipOnTheSameTrackReadsNoMetadataAgain() {
        val reads = mutableListOf<String>()
        val playing = map(snapshot(AndroidPlaybackState.PLAYING, 5))

        val paused = widgetNowPlaying(
            snapshot(AndroidPlaybackState.PAUSED, 5),
            playing,
            { uri -> reads += uri; metadataFor(uri) },
            { uri -> reads += uri; null },
        )

        assertEquals(emptyList<String>(), reads)
        assertEquals(playing.copy(isPlaying = false), paused)
    }

    @Test
    fun aNewTrackReplacesTheOldOneCompletely() {
        val first = map(snapshot(AndroidPlaybackState.PLAYING, 5))

        val second = map(snapshot(AndroidPlaybackState.PLAYING, 6), previous = first)

        assertEquals("Title 6", second.title)
        assertEquals("/cache/6.flac.png", second.artworkPath)
    }

    @Test
    fun aTrackTheLibraryDoesNotKnowStillGetsAWidgetWithoutText() {
        val state = widgetNowPlaying(
            snapshot(AndroidPlaybackState.PLAYING, 7),
            WidgetNowPlaying.Empty,
            { null },
            { null },
        )

        assertEquals(7L, state.trackId)
        assertEquals("", state.title)
        assertNull(state.artworkPath)
        assertFalse(state.isEmpty)
    }

    @Test
    fun onlyATrackChangeOrAPlayPauseFlipChangesTheKey() {
        val playing = snapshot(AndroidPlaybackState.PLAYING, 5, positionMs = 100).widgetKey()

        assertEquals(playing, snapshot(AndroidPlaybackState.PLAYING, 5, positionMs = 9_000).widgetKey())
        assertFalse(playing == snapshot(AndroidPlaybackState.PAUSED, 5).widgetKey())
        assertFalse(playing == snapshot(AndroidPlaybackState.PLAYING, 6).widgetKey())
    }
}
