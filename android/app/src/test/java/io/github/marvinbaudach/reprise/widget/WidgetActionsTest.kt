package io.github.marvinbaudach.reprise.widget

import android.content.Context
import androidx.glance.GlanceId
import androidx.glance.action.ActionParameters
import androidx.glance.action.actionParametersOf
import androidx.test.core.app.ApplicationProvider
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class WidgetActionsTest {
    private val context: Context = ApplicationProvider.getApplicationContext()
    private val noGlanceId = object : GlanceId {}

    private class Recording(
        command: WidgetCommand,
        private val sent: MutableList<WidgetCommand>,
        private val failure: Exception? = null,
    ) : WidgetCommandAction(command) {
        override fun sink(context: Context) = WidgetCommandSink { command ->
            sent += command
            failure?.let { throw it }
        }
    }

    private fun tap(action: WidgetCommandAction) = runBlocking {
        action.onAction(context, noGlanceId, actionParametersOf() as ActionParameters)
    }

    @Test
    fun eachButtonSendsItsOwnCommand() {
        assertEquals(WidgetCommand.PREVIOUS, PreviousAction().command)
        assertEquals(WidgetCommand.TOGGLE_PLAY, TogglePlayAction().command)
        assertEquals(WidgetCommand.NEXT, NextAction().command)
    }

    @Test
    fun aTapDeliversTheButtonsCommandToTheSink() {
        val sent = mutableListOf<WidgetCommand>()

        tap(Recording(WidgetCommand.NEXT, sent))

        assertEquals(listOf(WidgetCommand.NEXT), sent)
    }

    @Test
    fun aServiceThatCannotBeReachedDoesNotCrashTheTap() {
        val sent = mutableListOf<WidgetCommand>()

        tap(Recording(WidgetCommand.TOGGLE_PLAY, sent, failure = IllegalStateException("no service")))

        assertEquals(listOf(WidgetCommand.TOGGLE_PLAY), sent)
    }
}
