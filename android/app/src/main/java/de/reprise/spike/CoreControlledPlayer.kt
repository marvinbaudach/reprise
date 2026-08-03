package de.reprise.spike

import androidx.media3.common.ForwardingPlayer
import androidx.media3.common.Player

/** Routes MediaSession transport commands back through the Core session. */
internal class CoreControlledPlayer(
    player: Player,
    private val commands: Commands,
) : ForwardingPlayer(player) {
    internal interface Commands {
        fun togglePause()

        fun next()

        fun previous()
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
        commands.previous()
    }

    override fun seekToPreviousMediaItem() {
        commands.previous()
    }
}
