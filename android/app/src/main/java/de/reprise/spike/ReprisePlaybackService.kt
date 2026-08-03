package de.reprise.spike

import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.session.MediaSession
import androidx.media3.session.MediaSessionService

/** Owns Media3 for background playback, notifications and external controls. */
class ReprisePlaybackService : MediaSessionService() {
    private var mediaSession: MediaSession? = null
    internal var playbackPort: Media3PlaybackPort? = null
        private set

    override fun onCreate() {
        super.onCreate()
        val player = ExoPlayer.Builder(this).build()
        playbackPort = Media3PlaybackPort(player)
        mediaSession = MediaSession.Builder(this, player).build()
    }

    override fun onGetSession(
        controllerInfo: MediaSession.ControllerInfo,
    ): MediaSession? = mediaSession

    override fun onDestroy() {
        playbackPort?.release()
        playbackPort = null
        mediaSession?.release()
        mediaSession = null
        super.onDestroy()
    }
}
