package io.github.marvinbaudach.reprise.widget

import android.content.Context
import android.util.Log
import androidx.glance.GlanceId
import androidx.glance.action.ActionParameters
import androidx.glance.appwidget.action.ActionCallback
import io.github.marvinbaudach.reprise.ReprisePlaybackService

private const val TAG = "RepriseWidget"

/**
 * One class per button, because a Glance action callback is instantiated by
 * the framework by class and carries no constructor arguments.
 */
internal abstract class WidgetCommandAction(internal val command: WidgetCommand) : ActionCallback {
    override suspend fun onAction(context: Context, glanceId: GlanceId, parameters: ActionParameters) {
        try {
            sink(context).send(command)
        } catch (error: Exception) {
            Log.w(TAG, "The widget could not reach the playback service", error)
        }
    }

    protected open fun sink(context: Context): WidgetCommandSink =
        MediaControllerSink(context.applicationContext, ReprisePlaybackService::class.java)
}

internal class PreviousAction : WidgetCommandAction(WidgetCommand.PREVIOUS)

internal class TogglePlayAction : WidgetCommandAction(WidgetCommand.TOGGLE_PLAY)

internal class NextAction : WidgetCommandAction(WidgetCommand.NEXT)
