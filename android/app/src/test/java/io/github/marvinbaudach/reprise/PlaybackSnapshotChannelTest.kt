package io.github.marvinbaudach.reprise

import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.android.controller.ServiceController
import org.robolectric.annotation.Config
import uniffi.reprise_android_ffi.AndroidPlaybackSession
import uniffi.reprise_android_ffi.AndroidPlaybackSnapshot
import uniffi.reprise_android_ffi.AndroidPlaybackState
import uniffi.reprise_android_ffi.AndroidRepeatMode

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class PlaybackSnapshotChannelTest {
    private val services = mutableListOf<ServiceController<ChannelTestPlaybackService>>()

    @After
    fun releaseTheServices() {
        services.forEach(ServiceController<ChannelTestPlaybackService>::destroy)
    }

    @Test
    fun aSnapshotPublishedWithNobodyListeningIsStillReadableAfterwards() {
        val service = buildService()
        val snapshot = channelSnapshot(trackId = 41, positionMs = 12_000)

        service.coreListener.onPlaybackChanged(snapshot)

        assertEquals(snapshot, service.playbackSnapshots.value)
    }

    @Test
    fun aLateCollectorReceivesTheStateThatWasPublishedBeforeItArrived() = runBlocking {
        val service = buildService()
        val snapshot = channelSnapshot(trackId = 42, positionMs = 18_000)
        service.coreListener.onPlaybackChanged(snapshot)

        assertEquals(snapshot, service.playbackSnapshots.first())
    }

    private fun buildService(): ChannelTestPlaybackService =
        Robolectric.buildService(ChannelTestPlaybackService::class.java)
            .create()
            .also(services::add)
            .get()
}

private class ChannelTestPlaybackService : ReprisePlaybackService() {
    override fun openCoreSession(port: Media3PlaybackPort): AndroidPlaybackSession? = null
}

private fun channelSnapshot(trackId: Long, positionMs: Long) = AndroidPlaybackSnapshot(
    state = AndroidPlaybackState.PLAYING,
    currentIndex = 0UL,
    currentTrackId = trackId,
    currentTrackUri = "content://provider/document/$trackId.flac",
    positionMs = positionMs,
    durationMs = 180_000,
    automaticAdvanceCount = 0u,
    shuffled = false,
    repeat = AndroidRepeatMode.OFF,
    error = null,
)
