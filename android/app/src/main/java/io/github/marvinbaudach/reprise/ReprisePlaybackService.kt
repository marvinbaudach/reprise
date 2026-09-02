package io.github.marvinbaudach.reprise

import android.content.Intent
import android.net.Uri
import android.os.Binder
import android.os.IBinder
import android.os.Handler
import android.os.Looper
import androidx.media3.common.AudioAttributes
import androidx.media3.common.C
import androidx.media3.common.Player
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.exoplayer.audio.TeeAudioProcessor
import androidx.media3.session.MediaSession
import androidx.media3.session.MediaSessionService
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import uniffi.reprise_android_ffi.AndroidEqualizerSnapshot
import uniffi.reprise_android_ffi.AndroidPlaybackListener
import uniffi.reprise_android_ffi.AndroidPlaybackSession
import uniffi.reprise_android_ffi.AndroidPlaybackSnapshot
import uniffi.reprise_android_ffi.AndroidPlaybackState
import uniffi.reprise_android_ffi.AndroidRepeatMode
import uniffi.reprise_android_ffi.AndroidTrashReport
import uniffi.reprise_android_ffi.AndroidVisualEngine
import uniffi.reprise_android_ffi.TrashAction

/** Owns Media3 for background playback, notifications and external controls. */
open class ReprisePlaybackService : MediaSessionService() {
    private var mediaSession: MediaSession? = null
    private var playbackPort: Media3PlaybackPort? = null
    private var coreSession: AndroidPlaybackSession? = null
    private val mutablePlaybackSnapshots = MutableStateFlow<AndroidPlaybackSnapshot?>(null)
    internal val playbackSnapshots: StateFlow<AndroidPlaybackSnapshot?> =
        mutablePlaybackSnapshots.asStateFlow()
    private val mutableSettingsRevisions = MutableStateFlow(0L)
    internal val settingsRevisions: StateFlow<Long> = mutableSettingsRevisions.asStateFlow()
    private val mutableSleepTimerStates = MutableStateFlow(SleepTimerUiState())
    internal val sleepTimerStates: StateFlow<SleepTimerUiState> =
        mutableSleepTimerStates.asStateFlow()
    private lateinit var sleepTimer: SleepTimerController
    private val localBinder = LocalBinder()
    private val livePcmSink = LivePcmBufferSink()
    private var liveVisualEngine: NativeVisualSceneEngine? = null

    /**
     * The core's own callback, and the one place that learns playback has run
     * out. Visible to the tests because it is the surface the Rust side calls:
     * a test that reaches the same decision through a different door would be
     * proving something else.
     */
    internal val coreListener = object : AndroidPlaybackListener {
        override fun onPlaybackChanged(snapshot: AndroidPlaybackSnapshot) {
            mutablePlaybackSnapshots.value = snapshot
            if (::sleepTimer.isInitialized) sleepTimer.onPlaybackSnapshot(snapshot)
            if (snapshot.hasRunOut()) {
                // The queue is empty, so this service has nothing left to keep
                // alive. `stopSelf` only ends a service nobody is bound to, so
                // an open screen keeps its transport controls and can start
                // playback again; a screenless service goes away instead of
                // holding a player and a session for silence.
                stopSelf()
            }
        }

        override fun onListenReportChanged() {
            publishListenReport()
        }
    }
    private val mediaSessionCommands = object : CoreControlledPlayer.Commands {
        override fun togglePause() = this@ReprisePlaybackService.togglePause()

        override fun next() = this@ReprisePlaybackService.next()

        override fun previousInQueueOrder() = this@ReprisePlaybackService.previousInQueueOrder()
    }

    override fun onCreate() {
        super.onCreate()
        val player = ExoPlayer.Builder(
            this,
            LivePcmRenderersFactory(this, TeeAudioProcessor(livePcmSink)),
        )
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
        // Keep this listener ahead of Media3PlaybackPort: on resume it resets
        // the PCM history and advances the native stream generation before the
        // port publishes PLAYING through Core and Compose calls setPlaying.
        val port = createAfterLivePcmListener(player, livePcmSink) {
            player.trackSelectionParameters = player.trackSelectionParameters
                .buildUpon()
                .setAudioOffloadPreferences(livePcmAudioOffloadPreferences())
                .build()
            Media3PlaybackPort(player) { mutableSettingsRevisions.value += 1L }
        }
        playbackPort = port
        sleepTimer = SleepTimerController(
            handler = Handler(Looper.getMainLooper()),
            applyVolume = ::applySleepTimerVolume,
            pause = ::pauseForSleepTimer,
            publish = { state -> mutableSleepTimerStates.value = state },
        )
        mutableSleepTimerStates.value = sleepTimer.state()
        val session = MediaSession.Builder(
            this,
            CoreControlledPlayer(player, mediaSessionCommands, this),
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
        mutablePlaybackSnapshots.value = coreSession?.snapshot()
        publishListenReport()
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
        AndroidPlaybackSession(
            sharedMusicLibrary(),
            port,
            coreListener,
        )

    override fun onBind(intent: Intent): IBinder? =
        if (intent.action == LOCAL_BIND_ACTION) localBinder else super.onBind(intent)

    override fun onGetSession(
        controllerInfo: MediaSession.ControllerInfo,
    ): MediaSession? = mediaSession

    override fun onDestroy() {
        if (::sleepTimer.isInitialized) sleepTimer.close()
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
        livePcmSink.detachAll()
        liveVisualEngine?.close()
        liveVisualEngine = null
        super.onDestroy()
    }

    internal fun visualSceneEngineFactory(): VisualSceneEngineFactory =
        VisualSceneEngineFactory {
            val engine = liveVisualEngine ?: NativeVisualSceneEngine(AndroidVisualEngine()).also {
                liveVisualEngine = it
            }
            LiveVisualSceneEngineLease(engine, livePcmSink)
        }

    internal fun startSleepTimer(selection: SleepTimerSelection) {
        sleepTimer.start(selection, playbackSnapshots.value)
    }

    internal fun cancelSleepTimer() {
        sleepTimer.cancel()
    }

    internal fun sleepTimerState(): SleepTimerUiState = sleepTimer.state()

    internal open fun applySleepTimerVolume(volume: Float) {
        playbackPort?.setVolume(volume.toDouble())
    }

    internal open fun pauseForSleepTimer() {
        if (playbackSnapshots.value?.state == AndroidPlaybackState.PLAYING) {
            coreSession?.togglePause()
        }
    }

    internal fun sleepTimerPlaybackPositionMs(): Long =
        playbackSnapshots.value?.positionMs ?: 0L

    internal fun playTracks(tracks: List<LibraryTrack>, startIndex: Int) {
        coreSession().playTracks(
            tracks.map(LibraryTrack::id),
            tracks.map(LibraryTrack::uri),
            startIndex.toULong(),
        )
    }

    internal open fun playTrackIds(trackIds: List<Long>, startIndex: Int) {
        coreSession().playTrackIds(trackIds, startIndex.toULong())
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

    internal fun previousInQueueOrder() {
        coreSession().previousInQueueOrder()
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

    internal fun upcomingTracks(window: LibraryWindowRange): LibraryWindow<LibraryTrack> =
        coreSession().upcomingTracks(window.toFfi()).toLibraryTracks()

    internal fun playUpcomingTrackNow(position: Int, expectedTrackId: Long): Boolean =
        coreSession().playUpcomingTrackNow(position.toULong(), expectedTrackId)

    internal fun moveUpcomingTrack(
        fromPosition: Int,
        expectedTrackId: Long,
        toPosition: Int,
    ): Boolean = coreSession().moveUpcomingTrack(
        fromPosition.toULong(),
        expectedTrackId,
        toPosition.toULong(),
    )

    internal fun removeUpcomingTrack(position: Int, expectedTrackId: Long): Boolean =
        coreSession().removeUpcomingTrack(position.toULong(), expectedTrackId)

    internal open fun queueTracksNext(trackIds: List<Long>): UInt =
        coreSession().queueTracksNext(trackIds)

    internal open fun queueTracksLast(trackIds: List<Long>): UInt =
        coreSession().queueTracksLast(trackIds)

    internal open fun trashTracks(
        trackIds: List<Long>,
        action: TrashAction,
    ): AndroidTrashReport = coreSession().trashTracks(trackIds, action)

    private fun coreSession(): AndroidPlaybackSession = checkNotNull(coreSession) {
        "Core playback session is not ready"
    }

    private fun publishListenReport() {
        val session = coreSession ?: return
        val treeUri = getSharedPreferences("reprise_android", MODE_PRIVATE)
            .getString(TREE_URI_PREFERENCE, null)
            ?: return
        val files = AndroidListenReportFiles(contentResolver, Uri.parse(treeUri))
        ListenReportWriter(
            readAcknowledgement = files::readAcknowledgement,
            produceReport = session::prepareListenReport,
            writeReport = files::writeReport,
        ).publish().onFailure { error ->
            android.util.Log.w("ReprisePlayback", "Could not publish phone listening report", error)
        }
    }

    inner class LocalBinder : Binder() {
        internal fun service(): ReprisePlaybackService = this@ReprisePlaybackService
    }

    internal companion object {
        const val LOCAL_BIND_ACTION = "org.reprise.BIND_PLAYBACK"
    }
}

internal fun <T> createAfterLivePcmListener(
    player: Player,
    livePcmSink: LivePcmBufferSink,
    create: () -> T,
): T {
    player.addListener(livePcmSink)
    return create()
}

private class LiveVisualSceneEngineLease(
    private val engine: NativeVisualSceneEngine,
    private val sink: LivePcmBufferSink,
) : VisualSceneEngine by engine, LivePcmConsumer {
    init {
        sink.attach(this)
    }

    override fun ingestPcm16(
        bytes: ByteArray,
        byteCount: Int,
        sampleRateHz: Int,
        channelCount: Int,
    ) = engine.ingestPcm16(bytes, byteCount, sampleRateHz, channelCount)

    override fun setPlaybackIntent(playbackIntended: Boolean) =
        engine.setPlaybackIntent(playbackIntended)

    override fun resetAudioStream() = engine.resetAudioStream()

    override fun resetAudioHistory() = engine.resetAudioHistory()

    override fun close() {
        sink.detach(this)
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
