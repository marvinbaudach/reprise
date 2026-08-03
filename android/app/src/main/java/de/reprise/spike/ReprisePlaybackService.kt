package de.reprise.spike

import android.content.Intent
import android.os.Binder
import android.os.IBinder
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.session.MediaSession
import androidx.media3.session.MediaSessionService
import uniffi.reprise_android_ffi.AndroidPlaybackListener
import uniffi.reprise_android_ffi.AndroidPlaybackSession
import uniffi.reprise_android_ffi.AndroidPlaybackSnapshot

/** Owns Media3 for background playback, notifications and external controls. */
class ReprisePlaybackService : MediaSessionService() {
    private var mediaSession: MediaSession? = null
    private var playbackPort: Media3PlaybackPort? = null
    private var coreSession: AndroidPlaybackSession? = null
    private var observer: ((AndroidPlaybackSnapshot) -> Unit)? = null
    private val localBinder = LocalBinder()

    private val coreListener = object : AndroidPlaybackListener {
        override fun onPlaybackChanged(snapshot: AndroidPlaybackSnapshot) {
            observer?.invoke(snapshot)
        }
    }
    private val mediaSessionCommands = object : CoreControlledPlayer.Commands {
        override fun togglePause() = this@ReprisePlaybackService.togglePause()

        override fun next() = this@ReprisePlaybackService.next()

        override fun previous() = this@ReprisePlaybackService.previous()
    }

    override fun onCreate() {
        super.onCreate()
        val player = ExoPlayer.Builder(this).build()
        val port = Media3PlaybackPort(player)
        playbackPort = port
        coreSession = AndroidPlaybackSession(port, coreListener)
        mediaSession = MediaSession.Builder(
            this,
            CoreControlledPlayer(player, mediaSessionCommands),
        ).build()
    }

    override fun onBind(intent: Intent): IBinder? =
        if (intent.action == LOCAL_BIND_ACTION) localBinder else super.onBind(intent)

    override fun onGetSession(
        controllerInfo: MediaSession.ControllerInfo,
    ): MediaSession? = mediaSession

    override fun onDestroy() {
        observer = null
        coreSession?.close()
        coreSession = null
        mediaSession?.release()
        mediaSession = null
        playbackPort?.release()
        playbackPort = null
        super.onDestroy()
    }

    internal fun attachObserver(observer: (AndroidPlaybackSnapshot) -> Unit) {
        this.observer = observer
        coreSession?.snapshot()?.let(observer)
    }

    internal fun detachObserver() {
        observer = null
    }

    internal fun playTracks(uris: List<String>, startIndex: Int) {
        coreSession().playTracks(uris, startIndex.toULong())
    }

    internal fun togglePause() {
        coreSession().togglePause()
    }

    internal fun next() {
        coreSession().next()
    }

    internal fun previous() {
        coreSession().previous()
    }

    private fun coreSession(): AndroidPlaybackSession = checkNotNull(coreSession) {
        "Core playback session is not ready"
    }

    inner class LocalBinder : Binder() {
        internal fun service(): ReprisePlaybackService = this@ReprisePlaybackService
    }

    internal companion object {
        const val LOCAL_BIND_ACTION = "de.reprise.spike.BIND_PLAYBACK"
    }
}
