package io.github.marvinbaudach.reprise.widget

import android.content.ComponentName
import android.content.Context
import androidx.media3.common.Player
import androidx.media3.session.MediaController
import androidx.media3.session.SessionToken
import com.google.common.util.concurrent.ListenableFuture
import com.google.common.util.concurrent.MoreExecutors
import java.util.concurrent.TimeoutException
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.coroutines.withContext

/** What a widget button asks of the player. */
internal enum class WidgetCommand {
    PREVIOUS,
    TOGGLE_PLAY,
    NEXT,
}

/**
 * Sends [command] to [player]. Next and previous go through the session
 * player, which routes them into the Core's queue; nothing here knows how.
 */
internal fun applyWidgetCommand(player: Player, command: WidgetCommand) {
    when (command) {
        WidgetCommand.PREVIOUS -> player.seekToPrevious()
        WidgetCommand.NEXT -> player.seekToNext()
        WidgetCommand.TOGGLE_PLAY -> if (player.playWhenReady) player.pause() else player.play()
    }
}

/** Delivers a [WidgetCommand] to the playback service. */
internal fun interface WidgetCommandSink {
    suspend fun send(command: WidgetCommand)
}

/**
 * Reaches the service through a [MediaController], the same door the system's
 * own media controls use. It starts the service when it is not running, and a
 * tap on a widget is one of the interactions the system lets do that from the
 * background.
 */
internal class MediaControllerSink(
    private val context: Context,
    private val serviceClass: Class<*>,
) : WidgetCommandSink {
    override suspend fun send(command: WidgetCommand) {
        // A controller is tied to the looper it is built on; the main thread
        // has one, a widget callback's worker does not.
        withContext(Dispatchers.Main) {
            val token = SessionToken(context, ComponentName(context, serviceClass))
            val controller = awaitConnection(MediaController.Builder(context, token).buildAsync())
            try {
                applyWidgetCommand(controller, command)
            } finally {
                controller.release()
            }
        }
    }
}

/** How long a widget tap waits for the service to answer before it gives up. */
internal const val CONNECT_TIMEOUT_MS = 5_000L

/**
 * Waits for [future], at most [timeoutMs]. A service that never answers (a
 * process that is starting slowly, a session that died while connecting) must
 * not hold the widget's callback open for the system to time out, so the
 * request is cancelled and a [TimeoutException] says why nothing happened.
 */
internal suspend fun <T : Any> awaitConnection(
    future: ListenableFuture<T>,
    timeoutMs: Long = CONNECT_TIMEOUT_MS,
): T {
    val connected = withTimeoutOrNull(timeoutMs) {
        suspendCancellableCoroutine<T> { continuation ->
            future.addListener(
                {
                    try {
                        continuation.resume(future.get())
                    } catch (error: Exception) {
                        continuation.resumeWithException(error)
                    }
                },
                MoreExecutors.directExecutor(),
            )
            continuation.invokeOnCancellation { future.cancel(true) }
        }
    }
    return connected ?: run {
        future.cancel(true)
        throw TimeoutException("The playback service did not answer within $timeoutMs ms")
    }
}
