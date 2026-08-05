package de.reprise.spike

import android.content.Intent
import android.os.Binder
import android.os.IBinder
import androidx.media3.common.AudioAttributes
import androidx.media3.common.C
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.session.MediaSession
import androidx.media3.session.MediaSessionService
import uniffi.reprise_android_ffi.AndroidEqualizerSnapshot
import uniffi.reprise_android_ffi.AndroidPlaybackListener
import uniffi.reprise_android_ffi.AndroidPlaybackSession
import uniffi.reprise_android_ffi.AndroidPlaybackSnapshot
import uniffi.reprise_android_ffi.AndroidPlaybackState
import uniffi.reprise_android_ffi.AndroidRepeatMode

/** Owns Media3 for background playback, notifications and external controls. */
open class ReprisePlaybackService : MediaSessionService() {
    private var mediaSession: MediaSession? = null
    private var playbackPort: Media3PlaybackPort? = null
    private var coreSession: AndroidPlaybackSession? = null
    private var observer: ((AndroidPlaybackSnapshot) -> Unit)? = null
    private var settingsObserver: (() -> Unit)? = null
    private val localBinder = LocalBinder()

    /**
     * The core's own callback, and the one place that learns playback has run
     * out. Visible to the tests because it is the surface the Rust side calls:
     * a test that reaches the same decision through a different door would be
     * proving something else.
     */
    internal val coreListener = object : AndroidPlaybackListener {
        override fun onPlaybackChanged(snapshot: AndroidPlaybackSnapshot) {
            observer?.invoke(snapshot)
            if (snapshot.hasRunOut()) {
                // The queue is empty, so this service has nothing left to keep
                // alive. `stopSelf` only ends a service nobody is bound to, so
                // an open screen keeps its transport controls and can start
                // playback again; a screenless service goes away instead of
                // holding a player and a session for silence.
                stopSelf()
            }
        }
    }
    private val mediaSessionCommands = object : CoreControlledPlayer.Commands {
        override fun togglePause() = this@ReprisePlaybackService.togglePause()

        override fun next() = this@ReprisePlaybackService.next()

        override fun previous() = this@ReprisePlaybackService.previous()
    }

    override fun onCreate() {
        super.onCreate()
        val player = ExoPlayer.Builder(this)
            // Media3 defaults both of these off, and the device confirms it:
            // while a track was playing, the system's audio focus stack was
            // empty. Without focus the app talks over other players, keeps
            // going through a call, and drowns navigation prompts. Without the
            // becoming-noisy handler, unplugging headphones keeps the music
            // playing out loud through the speaker.
            .setAudioAttributes(
                AudioAttributes.Builder()
                    .setUsage(C.USAGE_MEDIA)
                    .setContentType(C.AUDIO_CONTENT_TYPE_MUSIC)
                    .build(),
                /* handleAudioFocus = */ true,
            )
            .setHandleAudioBecomingNoisy(true)
            .build()
        val port = Media3PlaybackPort(player) { settingsObserver?.invoke() }
        playbackPort = port
        val session = MediaSession.Builder(
            this,
            CoreControlledPlayer(player, mediaSessionCommands),
        ).build()
        mediaSession = session
        // Handing the session to the service is what puts Media3 in charge of
        // the notification and of the foreground lifetime. `addSession` is the
        // call that subscribes Media3's notification manager to this session,
        // and that manager is the only thing that raises the service into the
        // foreground once the player really plays. Without it the platform saw
        // the session (`dumpsys media_session` reported PLAYING) while the
        // service stayed a plain bound service — and a bound service dies with
        // its last client, which is exactly what a rotation is.
        addSession(session)
        coreSession = openCoreSession(port)
    }

    /**
     * Opens the core's playback session.
     *
     * Its own method because it is the one step that needs the native library.
     * `PlaybackServiceLifetimeTest` runs this service on the JVM, where that
     * library cannot load, and overrides this to leave the core out; everything
     * above it — the player, the session, and handing that session to Media3 —
     * runs for real there. The nullable result costs nothing: [coreSession] was
     * always nullable, and a command that arrives without it is refused with a
     * message rather than a crash.
     */
    internal open fun openCoreSession(port: Media3PlaybackPort): AndroidPlaybackSession? =
        AndroidPlaybackSession(filesDir.absolutePath, port, coreListener)

    override fun onBind(intent: Intent): IBinder? =
        if (intent.action == LOCAL_BIND_ACTION) localBinder else super.onBind(intent)

    override fun onGetSession(
        controllerInfo: MediaSession.ControllerInfo,
    ): MediaSession? = mediaSession

    override fun onDestroy() {
        observer = null
        settingsObserver = null
        coreSession?.close()
        coreSession = null
        mediaSession?.let { session ->
            // Unsubscribe Media3 before releasing: it holds this session in its
            // own map and would otherwise be left with a released one.
            removeSession(session)
            session.release()
        }
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

    internal fun attachSettingsObserver(observer: () -> Unit) {
        settingsObserver = observer
        observer()
    }

    internal fun detachSettingsObserver() {
        settingsObserver = null
    }

    internal fun playTracks(tracks: List<LibraryTrack>, startIndex: Int) {
        coreSession().playTracks(
            tracks.map(LibraryTrack::id),
            tracks.map(LibraryTrack::uri),
            startIndex.toULong(),
        )
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

    internal fun seekTo(positionMs: Long) {
        coreSession().seekTo(positionMs)
    }

    internal fun setShuffle(enabled: Boolean) {
        coreSession().setShuffle(enabled)
    }

    internal fun setRepeat(mode: AndroidRepeatMode) {
        coreSession().setRepeat(mode)
    }

    internal fun reloadPlaybackSettings() {
        coreSession().reloadPlaybackSettings()
    }

    internal fun equalizerSnapshot(): AndroidEqualizerSnapshot? =
        coreSession().equalizerSnapshot()

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

/**
 * Whether this snapshot is the end of playback rather than a gap inside it.
 *
 * The core clears its cursor when the queue runs out, and it publishes the next
 * track's snapshot only once it has adopted that track — so a stopped snapshot
 * without a track is the end, never the moment between two tracks.
 */
private fun AndroidPlaybackSnapshot.hasRunOut(): Boolean =
    state == AndroidPlaybackState.STOPPED && currentTrackId == null
