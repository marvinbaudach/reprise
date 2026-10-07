package io.github.marvinbaudach.reprise.widget

import android.content.Context
import androidx.test.core.app.ApplicationProvider
import java.util.concurrent.Executor
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.reprise_android_ffi.AndroidPlaybackState

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class WidgetPublisherTest {
    private val context: Context = ApplicationProvider.getApplicationContext()
    private val store = WidgetStateStore(context)
    private val queued = ArrayDeque<Runnable>()
    private var refreshes = 0
    private var metadataReads = 0
    private var failMetadata = false
    private var cover: String? = null
    private var clock = 0L
    private val publisher = WidgetPublisher(
        executor = Executor { queued.addLast(it) },
        store = store,
        metadata = { key ->
            metadataReads += 1
            if (failMetadata) error("library unavailable")
            metadataFor(key)
        },
        artworkPath = { cover },
        refresh = { refreshes += 1 },
        now = { clock },
    )

    @After
    fun forgetTheProcess() {
        WidgetStateStore.resetProcessState()
    }

    private fun drain() {
        while (queued.isNotEmpty()) queued.removeFirst().run()
    }

    @Test
    fun aNewTrackIsStoredAndTheWidgetIsRefreshed() {
        publisher.onSnapshot(snapshot(AndroidPlaybackState.PLAYING, 3))
        drain()

        assertEquals("Title 3", store.load().title)
        assertEquals(true, store.load().isPlaying)
        assertEquals(1, refreshes)
    }

    @Test
    fun positionTicksNeitherReadMetadataNorRefreshTheWidget() {
        publisher.onSnapshot(snapshot(AndroidPlaybackState.PLAYING, 3, positionMs = 0))
        drain()

        repeat(20) { tick ->
            publisher.onSnapshot(snapshot(AndroidPlaybackState.PLAYING, 3, positionMs = 500L * tick))
        }
        drain()

        assertEquals(1, refreshes)
        assertEquals(1, metadataReads)
        assertEquals(0, queued.size)
    }

    @Test
    fun pausingRefreshesTheWidgetWithoutReadingMetadataAgain() {
        publisher.onSnapshot(snapshot(AndroidPlaybackState.PLAYING, 3))
        drain()

        publisher.onSnapshot(snapshot(AndroidPlaybackState.PAUSED, 3))
        drain()

        assertEquals(2, refreshes)
        assertEquals(1, metadataReads)
        assertEquals(false, store.load().isPlaying)
    }

    @Test
    fun aLibraryThatFailsLeavesTheWidgetAsItWasAndDoesNotThrow() {
        failMetadata = true

        publisher.onSnapshot(snapshot(AndroidPlaybackState.PLAYING, 3))
        drain()

        assertEquals(0, refreshes)
        assertEquals(true, store.load().isEmpty)
    }

    @Test
    fun anUpdateThatFailedIsTriedAgainByALaterSnapshotOfTheSameState() {
        failMetadata = true
        publisher.onSnapshot(snapshot(AndroidPlaybackState.PLAYING, 3))
        drain()
        failMetadata = false

        clock += RETRY_DELAY_MS
        publisher.onSnapshot(snapshot(AndroidPlaybackState.PLAYING, 3, positionMs = 500))
        drain()

        assertEquals("Title 3", store.load().title)
        assertEquals(1, refreshes)
    }

    @Test
    fun aLibraryThatKeepsFailingIsNotAskedOnEveryPositionTick() {
        failMetadata = true
        publisher.onSnapshot(snapshot(AndroidPlaybackState.PLAYING, 3))
        drain()

        repeat(20) { tick ->
            clock += 100
            publisher.onSnapshot(snapshot(AndroidPlaybackState.PLAYING, 3, positionMs = 100L * tick))
        }
        drain()

        assertEquals(1, metadataReads)
    }

    @Test
    fun aStateStillBeingPublishedIsNotQueuedTwice() {
        publisher.onSnapshot(snapshot(AndroidPlaybackState.PLAYING, 3))
        publisher.onSnapshot(snapshot(AndroidPlaybackState.PLAYING, 3, positionMs = 500))

        assertEquals(1, queued.size)
    }

    @Test
    fun aCoverThatWasMissingAtFirstReachesTheWidgetWhenItLands() {
        publisher.onSnapshot(snapshot(AndroidPlaybackState.PLAYING, 3))
        drain()
        assertEquals(null, store.load().artworkPath)
        cover = "/cache/3.png"

        publisher.onArtworkAvailable()
        drain()

        assertEquals("/cache/3.png", store.load().artworkPath)
        assertEquals(2, refreshes)
    }

    @Test
    fun aCoverThatIsStillMissingRefreshesNothing() {
        publisher.onSnapshot(snapshot(AndroidPlaybackState.PLAYING, 3))
        drain()

        publisher.onArtworkAvailable()
        drain()

        assertEquals(1, refreshes)
    }

    @Test
    fun aSnapshotThatArrivesAfterTheExecutorShutDownIsDroppedNotThrown() {
        val closed = WidgetPublisher(
            executor = Executor { throw java.util.concurrent.RejectedExecutionException("shut down") },
            store = store,
            metadata = { key -> metadataFor(key) },
            artworkPath = { null },
            refresh = { refreshes += 1 },
        )

        closed.onSnapshot(snapshot(AndroidPlaybackState.PLAYING, 3))

        assertEquals(0, refreshes)
    }
}
