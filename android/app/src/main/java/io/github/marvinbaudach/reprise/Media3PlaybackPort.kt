package io.github.marvinbaudach.reprise

import android.net.Uri
import android.os.Handler
import android.os.Looper
import android.util.Log
import androidx.media3.common.C
import androidx.media3.common.MediaItem
import androidx.media3.common.PlaybackException
import androidx.media3.common.Player
import io.github.marvinbaudach.reprise.library.PlaybackItems
import io.github.marvinbaudach.reprise.library.TrackMetadataResolver
import java.io.File
import java.io.FileNotFoundException
import java.util.concurrent.Executor
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors
import java.util.concurrent.RejectedExecutionException
import uniffi.reprise_android_ffi.AndroidEqualizerBand
import uniffi.reprise_android_ffi.AndroidEqualizerBandCapability
import uniffi.reprise_android_ffi.AndroidEqualizerPoint
import uniffi.reprise_android_ffi.AndroidEqualizerSnapshot
import uniffi.reprise_android_ffi.AndroidPlaybackException
import uniffi.reprise_android_ffi.AndroidPlaybackPort
import uniffi.reprise_android_ffi.AndroidPlaybackState
import uniffi.reprise_android_ffi.AndroidPlayerEvent
import uniffi.reprise_android_ffi.AndroidTransitionMode
import uniffi.reprise_android_ffi.PlaybackEventBridge
import uniffi.reprise_android_ffi.PlaybackEventBridgeInterface
import uniffi.reprise_android_ffi.projectEqualizerCurve

private const val POSITION_INTERVAL_MS = 500L
private const val MAX_PLAYBACK_ERROR_CAUSES = 3
private const val MAX_PLAYBACK_ERROR_SUMMARY_LENGTH = 1_024
private const val TAG = "ReprisePlayback"

internal fun playbackErrorSummary(errorCodeName: String, error: Throwable): String {
    val detail = error.message ?: errorCodeName
    val summary = StringBuilder("$errorCodeName: $detail")
    var cause = error.cause
    var causeCount = 0
    val seen = mutableListOf<Throwable>(error)
    while (cause != null && causeCount < MAX_PLAYBACK_ERROR_CAUSES) {
        val current = cause
        if (seen.any { previous -> previous === current }) {
            break
        }
        seen += current
        summary.append(" — ${current.javaClass.simpleName}")
        current.message?.let { message -> summary.append(": $message") }
        cause = current.cause
        causeCount += 1
    }
    if (summary.length <= MAX_PLAYBACK_ERROR_SUMMARY_LENGTH) {
        return summary.toString()
    }
    val proposedEnd = MAX_PLAYBACK_ERROR_SUMMARY_LENGTH - 1
    val end = if (
        Character.isHighSurrogate(summary[proposedEnd - 1]) &&
        Character.isLowSurrogate(summary[proposedEnd])
    ) {
        proposedEnd - 1
    } else {
        proposedEnd
    }
    return summary.substring(0, end) + "…"
}

internal fun isMissingFilePlaybackError(error: PlaybackException): Boolean {
    if (error.errorCode == PlaybackException.ERROR_CODE_IO_FILE_NOT_FOUND) {
        return true
    }
    var cause = error.cause
    while (cause != null) {
        if (cause is FileNotFoundException) {
            return true
        }
        cause = cause.cause
    }
    return false
}

private val URI_SCHEME = Regex("^[a-zA-Z][a-zA-Z0-9+.-]*://")

/**
 * The playable URI for what Core hands over: a device path or a provider URI.
 *
 * A string is only a URI when it begins with a real `scheme://`. A local path
 * may contain a colon (`/Music/AC:DC/x.mp3`), which `Uri.parse` would read as a
 * scheme boundary, so everything else is a file path.
 */
internal fun playbackUri(path: String): Uri =
    if (!path.startsWith("/") && URI_SCHEME.containsMatchIn(path)) {
        Uri.parse(path)
    } else {
        Uri.fromFile(File(path))
    }

/**
 * Media3 implementation of the foreign half of Core's PlaybackBackend.
 *
 * Every item carries its track's metadata and cover, which the notification,
 * lock screen, Android Auto and the widget read from it. They come from a
 * blocking library read, so the player's own thread never makes one: when the
 * Core asks for playback on that thread (a notification's next button does) an
 * unknown track starts bare and is completed in place a moment later. A caller
 * on any other thread is already blocked and reads straight away.
 *
 * [metadataExecutor] runs those reads; the port makes and owns one when none
 * is given.
 */
internal class Media3PlaybackPort(
    private val player: Player,
    metadata: TrackMetadataResolver = TrackMetadataResolver.None,
    mediaIdOf: (trackId: Long) -> String? = { null },
    metadataExecutor: Executor? = null,
    private val trackGainSink: TrackGainAudioSink? = null,
    private val equalizerChanged: () -> Unit,
) : AndroidPlaybackPort {
    private val items = PlaybackItems(metadata, mediaIdOf)
    private val ownedExecutor: ExecutorService? =
        if (metadataExecutor == null) {
            Executors.newSingleThreadExecutor { task -> Thread(task, "reprise-metadata") }
        } else {
            null
        }
    private val metadataExecutor: Executor = metadataExecutor ?: checkNotNull(ownedExecutor)
    private val resolving = mutableSetOf<String>()
    private val handler = Handler(player.applicationLooper)
    private val dispatch = player.applicationLooper.dispatch(handler)
    private val deviceEqualizer =
        DeviceEqualizer(AndroidEqualizerEngineFactory, CoreEqualizerCurveProjector)
    private var eventBridge: PlaybackEventBridgeInterface? = null
    private var generation = 0UL
    private var nextUri: String? = null
    private var released = false
    private var nextGainDb = 0.0
    private var transitionMode = AndroidTransitionMode.GAPLESS
    private var lastState: AndroidPlaybackState? = null
    private var finishedGeneration: ULong? = null

    private val positionTicker = object : Runnable {
        override fun run() {
            if (player.isPlaying) {
                emit(
                    AndroidPlayerEvent.Position(
                        positionMs = player.currentPosition.coerceAtLeast(0),
                        durationMs = player.duration.knownDuration(),
                    ),
                )
            }
            if (lastState == AndroidPlaybackState.PLAYING ||
                lastState == AndroidPlaybackState.BUFFERING
            ) {
                handler.postDelayed(this, POSITION_INTERVAL_MS)
            }
        }
    }

    private val listener = object : Player.Listener {
        override fun onIsPlayingChanged(isPlaying: Boolean) {
            emitState()
            handler.removeCallbacks(positionTicker)
            if (isPlaying) {
                handler.post(positionTicker)
            }
        }

        override fun onPlaybackStateChanged(playbackState: Int) {
            emitState()
            if (playbackState == Player.STATE_ENDED && finishedGeneration != generation) {
                finishedGeneration = generation
                emit(AndroidPlayerEvent.TrackFinished)
            }
        }

        override fun onPlayWhenReadyChanged(playWhenReady: Boolean, reason: Int) {
            emitState()
        }

        override fun onMediaItemTransition(mediaItem: MediaItem?, reason: Int) {
            if (reason != Player.MEDIA_ITEM_TRANSITION_REASON_AUTO) {
                return
            }
            generation += 1UL
            finishedGeneration = null
            emit(AndroidPlayerEvent.AdvancedToNext)
            discardPlayedItems()
        }

        override fun onPlayerError(error: PlaybackException) {
            val summary = playbackErrorSummary(error.errorCodeName, error)
            Log.e(TAG, summary, error)
            emit(
                AndroidPlayerEvent.Error(
                    message = summary,
                    missing = isMissingFilePlaybackError(error),
                ),
            )
        }

        override fun onAudioSessionIdChanged(audioSessionId: Int) {
            deviceEqualizer.onAudioSessionChanged(audioSessionId)
            equalizerChanged()
        }
    }

    init {
        dispatch.call {
            player.addListener(listener)
            deviceEqualizer.onAudioSessionChanged(player.audioSessionId)
        }
    }

    override fun setEventBridge(bridge: PlaybackEventBridge) = dispatch.call {
        eventBridge = bridge
    }

    override fun playPath(path: String, gainDb: Double) =
        startWithGain(Uri.fromFile(File(path)).toString(), gainDb)

    override fun playUri(uri: String) = startWithGain(uri, 0.0)

    private fun startWithGain(uri: String, gainDb: Double) {
        ensureKnown(uri)
        dispatch.call {
            trackGainSink?.startPlaylist(gainDb, nextUri?.let { nextGainDb })
            start(itemFor(uri))
        }
    }

    override fun togglePause(): AndroidPlaybackState = dispatch.call {
        if (player.playWhenReady) {
            player.pause()
            AndroidPlaybackState.PAUSED
        } else {
            player.play()
            AndroidPlaybackState.PLAYING
        }
    }

    override fun seekTo(positionMs: Long) = dispatch.call {
        player.seekTo(positionMs.coerceAtLeast(0))
    }

    override fun setVolume(volume: Double) = dispatch.call {
        player.volume = volume.coerceIn(0.0, 1.0).toFloat()
    }

    override fun setEqualizer(enabled: Boolean, curve: List<AndroidEqualizerPoint>) = dispatch.call {
        deviceEqualizer.configure(
            enabled = enabled,
            curve = curve.map { point ->
                EqualizerCurvePoint(point.frequencyHz, point.gainDb)
            },
        )
        equalizerChanged()
    }

    override fun equalizerSnapshot(): AndroidEqualizerSnapshot? = dispatch.call {
        deviceEqualizer.snapshot()?.let { snapshot ->
            AndroidEqualizerSnapshot(
                enabled = snapshot.enabled,
                available = snapshot.available,
                bands = snapshot.bands.map { band ->
                    AndroidEqualizerBand(
                        frequencyHz = band.frequencyHz,
                        gainDb = band.gainDb,
                        minimumGainDb = band.minimumGainDb,
                        maximumGainDb = band.maximumGainDb,
                    )
                },
            )
        }
    }

    override fun setAudioEffects(): Unit = dispatch.call {
        throw AndroidPlaybackException.Unsupported(
            "audio effects are outside the Android playback slice",
        )
    }

    override fun setSpectrumEnabled(enabled: Boolean): Unit = dispatch.call {
        throw AndroidPlaybackException.Unsupported(
            "spectrum analysis is outside the Android playback slice",
        )
    }

    override fun stop() = dispatch.call {
        nextUri = null
        nextGainDb = 0.0
        trackGainSink?.clearPlaylist()
        player.stop()
        player.clearMediaItems()
    }

    override fun setNext(uri: String?, gainDb: Double) {
        uri?.let(::ensureKnown)
        dispatch.call {
            nextUri = uri
            nextGainDb = gainDb
            trackGainSink?.setNextGain(uri?.let { gainDb })
            applyNextItem()
        }
    }

    override fun setTransition(mode: AndroidTransitionMode) = dispatch.call {
        transitionMode = mode
        applyNextItem()
    }

    override fun currentGeneration(): ULong = dispatch.call { generation }

    fun release() = dispatch.call {
        released = true
        ownedExecutor?.shutdownNow()
        handler.removeCallbacks(positionTicker)
        player.removeListener(listener)
        deviceEqualizer.release()
        player.release()
        eventBridge = null
    }

    /** The item for [uri], with its playable URI read by [playbackUri] so a colon in a local path stays a path. */
    private fun itemFor(uri: String): MediaItem {
        val item = items.build(uri)
        return item.buildUpon().setUri(playbackUri(uri)).build()
    }

    private fun start(mediaItem: MediaItem) {
        generation += 1UL
        finishedGeneration = null
        lastState = null
        player.setMediaItem(mediaItem)
        if (transitionMode == AndroidTransitionMode.GAPLESS) {
            nextUri?.let { uri -> player.addMediaItem(itemFor(uri)) }
        }
        player.prepare()
        player.play()
    }

    private fun applyNextItem() {
        if (player.mediaItemCount == 0) {
            return
        }
        val afterCurrent = player.currentMediaItemIndex + 1
        if (afterCurrent < player.mediaItemCount) {
            player.removeMediaItems(afterCurrent, player.mediaItemCount)
        }
        if (transitionMode == AndroidTransitionMode.GAPLESS) {
            nextUri?.let { uri -> player.addMediaItem(itemFor(uri)) }
        }
    }

    /**
     * Remembers the cover of [uri] and gives every queued item with that uri
     * its cover. Remembered, so an item built for the same track later (a
     * replay, the gapless next item) is born with it. The item is updated in
     * place: only its metadata changes, so ExoPlayer keeps the source it is
     * already playing and the notification, lock screen and widget pick the
     * cover up from the metadata change.
     */
    fun attachArtwork(uri: String, artwork: Uri) = dispatch.call {
        items.rememberCover(uri, artwork)
        refreshQueued(uri)
    }

    /** Replaces each queued item for [uri] with a rebuild from what is known now, if that differs. */
    private fun refreshQueued(uri: String) {
        val rebuilt = itemFor(uri)
        for (index in 0 until player.mediaItemCount) {
            val item = player.getMediaItemAt(index)
            if (item.localConfiguration?.uri == playbackUri(uri) && item != rebuilt) {
                player.replaceMediaItem(index, rebuilt)
            }
        }
    }

    /**
     * Makes sure what is known about [uri] is as complete as it can be before an
     * item is built: read now by a caller that is blocked anyway, scheduled
     * for the player's own thread, which must not wait for the library.
     */
    private fun ensureKnown(uri: String) {
        if (items.isKnown(uri)) return
        if (Looper.myLooper() != player.applicationLooper) {
            items.resolve(uri)
            return
        }
        if (!resolving.add(uri)) return
        try {
            metadataExecutor.execute {
                val found = items.resolve(uri)
                handler.post {
                    resolving.remove(uri)
                    if (found && !released) refreshQueued(uri)
                }
            }
        } catch (error: RejectedExecutionException) {
            // The port is being released; nothing is left to decorate.
            resolving.remove(uri)
        }
    }

    private fun discardPlayedItems() {
        val current = player.currentMediaItemIndex
        if (current > 0) {
            player.removeMediaItems(0, current)
        }
    }

    private fun emitState() {
        val state = media3PlaybackState(
            isPlaying = player.isPlaying,
            playWhenReady = player.playWhenReady,
            playbackState = player.playbackState,
        )
        if (state != lastState) {
            lastState = state
            emit(AndroidPlayerEvent.StateChanged(state))
        }
    }

    private fun emit(event: AndroidPlayerEvent) {
        eventBridge?.emit(generation, event)
    }
}

internal fun media3PlaybackState(
    isPlaying: Boolean,
    playWhenReady: Boolean,
    playbackState: Int,
): AndroidPlaybackState = when {
    isPlaying -> AndroidPlaybackState.PLAYING
    playbackState == Player.STATE_IDLE || playbackState == Player.STATE_ENDED ->
        AndroidPlaybackState.STOPPED
    playbackState == Player.STATE_BUFFERING && playWhenReady -> AndroidPlaybackState.BUFFERING
    else -> AndroidPlaybackState.PAUSED
}

/**
 * The one implementation of the curve projection, borrowed from the core.
 *
 * It lives here rather than in `DeviceEqualizer.kt` because this file is where
 * the native boundary already is: the equalizer itself stays testable on the
 * JVM without the `.so`.
 */
private object CoreEqualizerCurveProjector : EqualizerCurveProjector {
    override fun project(
        curve: List<EqualizerCurvePoint>,
        bands: List<DeviceEqualizerBandCapability>,
    ): List<Double> = projectEqualizerCurve(
        curve.map { point -> AndroidEqualizerPoint(point.frequencyHz, point.gainDb) },
        bands.map { band ->
            AndroidEqualizerBandCapability(
                frequencyHz = band.frequencyHz,
                minimumGainDb = band.minimumGainDb,
                maximumGainDb = band.maximumGainDb,
            )
        },
    ).map { projected -> projected.gainDb }
}

private fun Looper.dispatch(handler: Handler): ApplicationLooperDispatch =
    ApplicationLooperDispatch(
        isApplicationThread = { Looper.myLooper() == this },
        post = { command -> handler.post(command) },
    )

private fun Long.knownDuration(): Long = if (this == C.TIME_UNSET) 0 else coerceAtLeast(0)
