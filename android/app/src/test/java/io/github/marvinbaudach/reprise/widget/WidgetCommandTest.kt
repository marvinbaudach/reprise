package io.github.marvinbaudach.reprise.widget

import io.github.marvinbaudach.reprise.library.ItemListPlayer
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class WidgetCommandTest {
    private val fake = ItemListPlayer()

    private fun commands() = fake.calls.filter {
        it in setOf("play", "pause", "seekToNext", "seekToPrevious")
    }

    @Test
    fun nextAndPreviousMapToTheSessionPlayersSkips() {
        applyWidgetCommand(fake.player, WidgetCommand.NEXT)
        applyWidgetCommand(fake.player, WidgetCommand.PREVIOUS)

        assertEquals(listOf("seekToNext", "seekToPrevious"), commands())
    }

    @Test
    fun theToggleStartsAPausedPlayer() {
        fake.playWhenReady = false

        applyWidgetCommand(fake.player, WidgetCommand.TOGGLE_PLAY)

        assertEquals(listOf("play"), commands())
        assertTrue(fake.playWhenReady)
    }

    @Test
    fun theToggleStopsAPlayingPlayer() {
        fake.playWhenReady = true

        applyWidgetCommand(fake.player, WidgetCommand.TOGGLE_PLAY)

        assertEquals(listOf("pause"), commands())
    }
}
