package de.reprise.spike

import android.net.Uri
import android.os.Handler
import android.os.Looper
import androidx.media3.common.C
import androidx.media3.common.MediaItem
import androidx.media3.common.PlaybackException
import androidx.media3.common.Player
import java.io.File
import uniffi.reprise_android_ffi.AndroidEqualizerBand
import uniffi.reprise_android_ffi.AndroidEqualizerPoint
import uniffi.reprise_android_ffi.AndroidEqualizerSnapshot
import uniffi.reprise_android_ffi.AndroidPlaybackException
import uniffi.reprise_android_ffi.AndroidPlaybackPort
import uniffi.reprise_android_ffi.AndroidPlaybackState
import uniffi.reprise_android_ffi.AndroidPlayerEvent
import uniffi.reprise_android_ffi.AndroidTransitionMode
import uniffi.reprise_android_ffi.PlaybackEventBridge
import uniffi.reprise_android_ffi.PlaybackEventBridgeInterface

private const val POSITION_INTERVAL_MS = 500L

/** Media3 implementation of the foreign half of Core's PlaybackBackend. */
internal class Media3PlaybackPort(
    private val player: Player,
    private val equalizerChanged: () -> Unit,
) : AndroidPlaybackPort {
    private val handler = Handler(player.applicationLooper)
    private val dispatch = player.applicationLooper.dispatch(handler)
    private val deviceEqualizer = DeviceEqualizer(AndroidEqualizerEngineFactory)
    private var eventBridge: PlaybackEventBridgeInterface? = null
    private var generation = 0UL
    private var nextUri: String? = null
    private var transitionMode = AndroidTransitionMode.GAPLESS
    private var lastState: AndroidPlaybackState? = null
    private var finishedGeneration: ULong? = null

    private val positionTicker = object : Runnable {
        override fun run() {
            if (!player.isPlaying) {
                return
            }
            emit(
                AndroidPlayerEvent.Position(
                    positionMs = player.currentPosition.coerceAtLeast(0),
                    durationMs = player.duration.knownDuration(),
                ),
            )
            handler.postDelayed(this, POSITION_INTERVAL_MS)
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
            val detail = error.message ?: error.errorCodeName
            emit(AndroidPlayerEvent.Error("${error.errorCodeName}: $detail"))
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

    override fun playPath(path: String) = dispatch.call {
        start(MediaItem.fromUri(Uri.fromFile(File(path))))
    }

    override fun playUri(uri: String) = dispatch.call {
        start(MediaItem.fromUri(Uri.parse(uri)))
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
        player.stop()
        player.clearMediaItems()
    }

    override fun setNext(uri: String?) = dispatch.call {
        nextUri = uri
        applyNextItem()
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
            nextUri?.let { player.addMediaItem(MediaItem.fromUri(it)) }
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
            nextUri?.let { player.addMediaItem(MediaItem.fromUri(it)) }
        }
    }

    private fun discardPlayedItems() {
        val current = player.currentMediaItemIndex
        if (current > 0) {
            player.removeMediaItems(0, current)
        }
    }

    private fun emitState() {
        val state = when {
            player.isPlaying -> AndroidPlaybackState.PLAYING
            player.playbackState == Player.STATE_IDLE ||
                player.playbackState == Player.STATE_ENDED -> AndroidPlaybackState.STOPPED
            else -> AndroidPlaybackState.PAUSED
        }
        if (state != lastState) {
            lastState = state
            emit(AndroidPlayerEvent.StateChanged(state))
        }
    }

    private fun emit(event: AndroidPlayerEvent) {
        eventBridge?.emit(generation, event)
    }
}

private fun Looper.dispatch(handler: Handler): ApplicationLooperDispatch =
    ApplicationLooperDispatch(
        isApplicationThread = { Looper.myLooper() == this },
        post = { command -> handler.post(command) },
    )

private fun Long.knownDuration(): Long = if (this == C.TIME_UNSET) 0 else coerceAtLeast(0)
