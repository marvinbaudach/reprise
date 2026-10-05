package io.github.marvinbaudach.reprise

import android.net.Uri
import android.os.Handler
import android.os.Looper
import android.util.Log
import androidx.media3.common.C
import androidx.media3.common.MediaItem
import androidx.media3.common.PlaybackException
import androidx.media3.common.Player
import java.io.File
import io.github.marvinbaudach.reprise.library.TrackMetadataResolver
import io.github.marvinbaudach.reprise.library.playbackMediaItem
import java.io.FileNotFoundException
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

/** Media3 implementation of the foreign half of Core's PlaybackBackend. */
internal class Media3PlaybackPort(
    private val player: Player,
    private val metadata: TrackMetadataResolver = TrackMetadataResolver.None,
    private val equalizerChanged: () -> Unit,
) : AndroidPlaybackPort {
    private val handler = Handler(player.applicationLooper)
    private val dispatch = player.applicationLooper.dispatch(handler)
    private val deviceEqualizer =
        DeviceEqualizer(AndroidEqualizerEngineFactory, CoreEqualizerCurveProjector)
    private var eventBridge: PlaybackEventBridgeInterface? = null
    private var generation = 0UL
    private var nextItem: MediaItem? = null
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

    override fun playPath(path: String) {
        // Resolved before the hop to the player's thread: a library read has no
        // business holding that thread, and the caller is already blocked.
        val item = mediaItemFor(Uri.fromFile(File(path)).toString())
        dispatch.call { start(item) }
    }

    override fun playUri(uri: String) {
        val item = mediaItemFor(uri)
        dispatch.call { start(item) }
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
        nextItem = null
        player.stop()
        player.clearMediaItems()
    }

    override fun setNext(uri: String?) {
        val item = uri?.let(::mediaItemFor)
        dispatch.call {
            nextItem = item
            applyNextItem()
        }
    }

    override fun setTransition(mode: AndroidTransitionMode) = dispatch.call {
        transitionMode = mode
        applyNextItem()
    }

    override fun currentGeneration(): ULong = dispatch.call { generation }

    fun release() = dispatch.call {
        handler.removeCallbacks(positionTicker)
        player.removeListener(listener)
        deviceEqualizer.release()
        player.release()
        eventBridge = null
    }

    private fun start(mediaItem: MediaItem) {
        generation += 1UL
        finishedGeneration = null
        lastState = null
        player.setMediaItem(mediaItem)
        if (transitionMode == AndroidTransitionMode.GAPLESS) {
            nextItem?.let { item -> player.addMediaItem(item) }
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
            nextItem?.let { item -> player.addMediaItem(item) }
        }
    }

    /**
     * Gives every queued item with this uri its cover. The item is updated in
     * place: only its metadata changes, so ExoPlayer keeps the source it is
     * already playing and the notification, lock screen and widget pick the
     * cover up from the metadata change.
     */
    fun attachArtwork(uri: String, artwork: Uri) = dispatch.call {
        for (index in 0 until player.mediaItemCount) {
            val item = player.getMediaItemAt(index)
            if (item.localConfiguration?.uri?.toString() != uri || item.mediaMetadata.artworkUri != null) {
                continue
            }
            player.replaceMediaItem(
                index,
                item.buildUpon()
                    .setMediaMetadata(item.mediaMetadata.buildUpon().setArtworkUri(artwork).build())
                    .build(),
            )
        }
    }

    private fun mediaItemFor(uri: String): MediaItem {
        val track = try {
            metadata.resolve(uri)
        } catch (error: Exception) {
            // Metadata is decoration: a library that cannot answer must not
            // stop the music, only leave the notification without a title.
            Log.w(TAG, "Could not read the metadata for a playing track", error)
            null
        }
        return playbackMediaItem(uri, track)
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
