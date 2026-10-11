package io.github.marvinbaudach.reprise.widget

import android.os.Handler
import android.os.Looper
import io.github.marvinbaudach.reprise.FOG_CROSSFADE_MS
import io.github.marvinbaudach.reprise.NOW_PLAYING_SETTLE_MS
import java.time.Duration
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import uniffi.reprise_android_ffi.AndroidPlaybackSnapshot
import uniffi.reprise_android_ffi.AndroidPlaybackState

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class WidgetUpdateSettleTest {
    private val looper = shadowOf(Looper.getMainLooper())
    private val delivered = mutableListOf<AndroidPlaybackSnapshot?>()
    private val settle = WidgetUpdateSettle(
        handler = Handler(Looper.getMainLooper()),
        settleMs = WIDGET_UPDATE_SETTLE_MS,
        deliver = { delivered += it },
    )

    private fun advance(ms: Long) = looper.idleFor(Duration.ofMillis(ms))

    @Test
    fun theWidgetWaitsLongerThanTheSwipeAndTheFogCrossfade() {
        assertTrue(WIDGET_UPDATE_SETTLE_MS > NOW_PLAYING_SETTLE_MS + FOG_CROSSFADE_MS)
    }

    @Test
    fun aSingleChangeIsDeliveredOnlyOnceItHasSettled() {
        settle.onSnapshot(snapshot(AndroidPlaybackState.PLAYING, 3))

        advance(WIDGET_UPDATE_SETTLE_MS - 1)
        assertEquals(0, delivered.size)

        advance(1)
        assertEquals(listOf(3L), delivered.map { it?.currentTrackId })
    }

    @Test
    fun quickTrackChangesDeliverOnlyTheLastOne() {
        settle.onSnapshot(snapshot(AndroidPlaybackState.PLAYING, 1))
        advance(WIDGET_UPDATE_SETTLE_MS / 2)
        settle.onSnapshot(snapshot(AndroidPlaybackState.PLAYING, 2))
        advance(WIDGET_UPDATE_SETTLE_MS / 2)
        settle.onSnapshot(snapshot(AndroidPlaybackState.PLAYING, 3))
        advance(WIDGET_UPDATE_SETTLE_MS - 1)
        assertEquals(0, delivered.size)

        advance(1)

        assertEquals(listOf(3L), delivered.map { it?.currentTrackId })
    }

    @Test
    fun aTrackChangeThroughBufferingIsOneKeyChange() {
        settle.onSnapshot(snapshot(AndroidPlaybackState.PLAYING, 2))
        advance(WIDGET_UPDATE_SETTLE_MS)
        delivered.clear()

        settle.onSnapshot(snapshot(AndroidPlaybackState.BUFFERING, 3))
        advance(WIDGET_UPDATE_SETTLE_MS / 2)
        settle.onSnapshot(snapshot(AndroidPlaybackState.PLAYING, 3))
        advance(WIDGET_UPDATE_SETTLE_MS / 2 - 1)
        assertEquals("the buffering blip must not restart the wait", 0, delivered.size)

        advance(1)

        assertEquals(AndroidPlaybackState.PLAYING, delivered.single()?.state)
    }

    @Test
    fun theLatestSnapshotOfTheSettledKeyIsTheOneDelivered() {
        settle.onSnapshot(snapshot(AndroidPlaybackState.PLAYING, 3, positionMs = 0))
        settle.onSnapshot(snapshot(AndroidPlaybackState.PLAYING, 3, positionMs = 700))

        advance(WIDGET_UPDATE_SETTLE_MS)

        assertEquals(700L, delivered.single()?.positionMs)
    }

    @Test
    fun snapshotsOfASettledKeyPassStraightThroughSoARetryStillSeesThem() {
        settle.onSnapshot(snapshot(AndroidPlaybackState.PLAYING, 3))
        advance(WIDGET_UPDATE_SETTLE_MS)
        delivered.clear()

        settle.onSnapshot(snapshot(AndroidPlaybackState.PLAYING, 3, positionMs = 500))

        assertEquals(1, delivered.size)
    }

    @Test
    fun aPauseWithoutATrackChangeIsDeliveredAfterTheSameWait() {
        settle.onSnapshot(snapshot(AndroidPlaybackState.PLAYING, 3))
        advance(WIDGET_UPDATE_SETTLE_MS)
        delivered.clear()

        settle.onSnapshot(snapshot(AndroidPlaybackState.PAUSED, 3))
        advance(WIDGET_UPDATE_SETTLE_MS - 1)
        assertEquals(0, delivered.size)
        advance(1)

        assertEquals(AndroidPlaybackState.PAUSED, delivered.single()?.state)
    }

    @Test
    fun cancellingDropsTheWaitingUpdate() {
        settle.onSnapshot(snapshot(AndroidPlaybackState.PLAYING, 3))

        settle.cancel()
        advance(WIDGET_UPDATE_SETTLE_MS * 2)

        assertEquals(0, delivered.size)
    }
}
