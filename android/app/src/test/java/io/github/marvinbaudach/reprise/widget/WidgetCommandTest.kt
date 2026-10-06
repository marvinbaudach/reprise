package io.github.marvinbaudach.reprise.widget

import com.google.common.util.concurrent.SettableFuture
import io.github.marvinbaudach.reprise.library.ItemListPlayer
import java.util.concurrent.TimeoutException
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
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

    @Test
    fun aServiceThatNeverAnswersIsGivenUpOnAndTheRequestIsCancelled() {
        val never = SettableFuture.create<String>()

        assertThrows(TimeoutException::class.java) {
            runBlocking { awaitConnection(never, timeoutMs = 20) }
        }

        assertTrue(never.isCancelled)
    }

    @Test
    fun aServiceThatAnswersInTimeIsUsed() {
        val answered = SettableFuture.create<String>().apply { set("controller") }

        assertEquals("controller", runBlocking { awaitConnection(answered, timeoutMs = 1_000) })
    }
}
