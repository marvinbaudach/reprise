package io.github.marvinbaudach.reprise

import org.junit.Assert.assertFalse
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

class NetworkReturnTest {
    @Test
    fun net_7b_an_offline_to_online_transition_reports_one_network_return() {
        val detector = NetworkReturnDetector()

        assertFalse(detector.observe(online = false))
        assertTrue(detector.observe(online = true))
        assertFalse(detector.observe(online = true))
    }

    @Test
    fun a_cold_start_offline_uses_that_state_as_the_baseline() {
        val detector = NetworkReturnDetector()

        assertFalse(detector.observe(online = false))
        assertTrue(detector.observe(online = true))
    }

    @Test
    fun switching_between_online_default_networks_is_not_a_return() {
        val detector = NetworkReturnDetector()

        assertFalse(detector.observe(online = true))
        assertFalse(detector.observe(online = true))
    }

    @Test
    fun a_return_across_stop_and_start_is_kept_in_the_view_model() {
        val surface = MobileSurfaceViewModel()
        val detectorBeforeStop = surface.networkReturnDetector
        assertFalse(detectorBeforeStop.observe(online = false))

        val detectorAfterStart = surface.networkReturnDetector

        assertSame(detectorBeforeStop, detectorAfterStart)
        assertTrue(detectorAfterStart.observe(online = true))
    }
}
