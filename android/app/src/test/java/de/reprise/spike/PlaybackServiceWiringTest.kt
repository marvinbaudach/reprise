package de.reprise.spike

import androidx.media3.common.Player
import java.lang.reflect.Proxy
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Test

class PlaybackServiceWiringTest {
    @Test
    fun livePcmListenerIsRegisteredBeforeThePlaybackPortIsConstructed() {
        val events = mutableListOf<String>()
        val sink = LivePcmBufferSink()
        val player = Proxy.newProxyInstance(
            Player::class.java.classLoader,
            arrayOf(Player::class.java),
        ) { _, method, arguments ->
            if (method.name == "addListener") {
                assertSame(sink, arguments?.single())
                events += "listener"
            }
            null
        } as Player

        val port = createAfterLivePcmListener(player, sink) {
            events += "port"
            "constructed"
        }

        assertEquals("constructed", port)
        assertEquals(listOf("listener", "port"), events)
    }
}
