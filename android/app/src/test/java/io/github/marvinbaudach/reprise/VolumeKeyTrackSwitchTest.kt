package io.github.marvinbaudach.reprise

import org.junit.Assert.assertEquals
import org.junit.Test

class VolumeKeyTrackSwitchTest {
    @Test
    fun playingFirstPressStartsTrackingAndItsRepeatsAreConsumed() {
        val switch = VolumeKeyTrackSwitch(isPlaying = { true })

        assertEquals(VolumeKeyAction.StartTracking, switch.onDown(VolumeKey.UP, true))
        assertEquals(VolumeKeyAction.Ignore, switch.onDown(VolumeKey.UP, false))
    }

    @Test
    fun longPressSkipsInTheDirectionOfTheConsumedKey() {
        val switch = VolumeKeyTrackSwitch(isPlaying = { true })

        switch.onDown(VolumeKey.UP, true)
        assertEquals(VolumeKeyAction.SkipNext, switch.onLongPress(VolumeKey.UP))

        switch.onDown(VolumeKey.DOWN, true)
        assertEquals(VolumeKeyAction.SkipPrevious, switch.onLongPress(VolumeKey.DOWN))
    }

    @Test
    fun trackedShortPressRestoresOneVolumeStep() {
        val switch = VolumeKeyTrackSwitch(isPlaying = { true })

        switch.onDown(VolumeKey.DOWN, true)

        assertEquals(
            VolumeKeyAction.AdjustVolume(VolumeKey.DOWN),
            switch.onUp(VolumeKey.DOWN, wasTracking = true, wasCanceled = false),
        )
    }

    @Test
    fun canceledUpAfterLongPressIsConsumedWithoutAdjustingVolume() {
        val switch = VolumeKeyTrackSwitch(isPlaying = { true })

        switch.onDown(VolumeKey.UP, true)
        switch.onLongPress(VolumeKey.UP)

        assertEquals(
            VolumeKeyAction.Ignore,
            switch.onUp(VolumeKey.UP, wasTracking = true, wasCanceled = true),
        )
    }

    @Test
    fun forgettingAConsumedPressMakesItsUpEventPassThrough() {
        val switch = VolumeKeyTrackSwitch(isPlaying = { true })

        switch.onDown(VolumeKey.UP, true)
        switch.forget()

        assertEquals(
            VolumeKeyAction.Passthrough,
            switch.onUp(VolumeKey.UP, wasTracking = true, wasCanceled = false),
        )
    }

    @Test
    fun everyPhasePassesThroughWhenPlaybackWasNotPlayingAtDown() {
        val switch = VolumeKeyTrackSwitch(isPlaying = { false })

        assertEquals(VolumeKeyAction.Passthrough, switch.onDown(VolumeKey.DOWN, true))
        assertEquals(VolumeKeyAction.Passthrough, switch.onLongPress(VolumeKey.DOWN))
        assertEquals(
            VolumeKeyAction.Passthrough,
            switch.onUp(VolumeKey.DOWN, wasTracking = false, wasCanceled = false),
        )
    }

    @Test
    fun playbackStartingAfterPassthroughDownDoesNotConsumeTheUp() {
        var playing = false
        val switch = VolumeKeyTrackSwitch(isPlaying = { playing })

        assertEquals(VolumeKeyAction.Passthrough, switch.onDown(VolumeKey.UP, true))
        playing = true

        assertEquals(
            VolumeKeyAction.Passthrough,
            switch.onUp(VolumeKey.UP, wasTracking = false, wasCanceled = false),
        )
    }
}
