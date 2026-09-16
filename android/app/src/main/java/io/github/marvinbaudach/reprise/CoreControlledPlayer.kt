package io.github.marvinbaudach.reprise

import androidx.media3.common.ForwardingPlayer
import androidx.media3.common.Player
import androidx.media3.common.util.UnstableApi

/** Routes MediaSession transport commands back through the Core session. */
// ForwardingPlayer and its overrides are unstable in media3 1.11; one opt-in
// covers the class instead of a baseline entry per override.
@androidx.annotation.OptIn(UnstableApi::class)
internal class CoreControlledPlayer(
    player: Player,
    private val commands: Commands,
) : ForwardingPlayer(player) {
    internal interface Commands {
        fun togglePause()

        fun next()

        fun previousInQueueOrder()
    }

    override fun play() {
        if (!wrappedPlayer.playWhenReady) {
            commands.togglePause()
        }
    }

    override fun pause() {
        if (wrappedPlayer.playWhenReady) {
            commands.togglePause()
        }
    }

    override fun setPlayWhenReady(playWhenReady: Boolean) {
        if (playWhenReady != wrappedPlayer.playWhenReady) {
            commands.togglePause()
        }
    }

    override fun seekToNext() {
        commands.next()
    }

    override fun seekToNextMediaItem() {
        commands.next()
    }

    override fun seekToPrevious() {
        commands.previousInQueueOrder()
    }

    override fun seekToPreviousMediaItem() {
        commands.previousInQueueOrder()
    }
}
