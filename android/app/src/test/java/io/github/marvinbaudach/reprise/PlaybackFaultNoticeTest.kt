package io.github.marvinbaudach.reprise

import androidx.media3.common.PlaybackException
import java.io.FileNotFoundException
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class PlaybackFaultNoticeTest {
    @Test
    fun firstSnapshotArmsTheFaultNoticeWithoutRaisingIt() {
        val update = updatePlaybackFaultNotice(
            previousCount = null,
            currentCount = 4uL,
            text = "Track unavailable — skipped",
            previousMessage = null,
        )

        assertEquals(4uL, update.observedCount)
        assertNull(update.message)
    }

    @Test
    fun aLaterFaultNoticeCountRaisesTheSnapshotText() {
        val update = updatePlaybackFaultNotice(
            previousCount = 4uL,
            currentCount = 5uL,
            text = "Track unavailable — skipped",
            previousMessage = null,
        )

        assertEquals(5uL, update.observedCount)
        assertEquals(TransientMessage("Track unavailable — skipped"), update.message)
    }

    @Test
    fun repeatedFaultTextGetsItsOwnDismissalLifetime() {
        val first = updatePlaybackFaultNotice(
            previousCount = 4uL,
            currentCount = 5uL,
            text = "Track unavailable — skipped",
            previousMessage = null,
        ).message
        val second = updatePlaybackFaultNotice(
            previousCount = 5uL,
            currentCount = 6uL,
            text = "Track unavailable — skipped",
            previousMessage = first,
        ).message

        assertEquals(first?.text, second?.text)
        assertNotEquals(first, second)
    }

    @Test
    fun aMissingFileIsClassifiedFromTheMedia3CodeOrItsCauseChain() {
        val typed = PlaybackException(
            "Missing",
            null,
            PlaybackException.ERROR_CODE_IO_FILE_NOT_FOUND,
        )
        val caused = PlaybackException(
            "Source error",
            IllegalStateException("wrapper", FileNotFoundException("gone")),
            PlaybackException.ERROR_CODE_IO_UNSPECIFIED,
        )
        val other = PlaybackException(
            "Decoder error",
            IllegalStateException("broken"),
            PlaybackException.ERROR_CODE_DECODING_FAILED,
        )

        assertTrue(isMissingFilePlaybackError(typed))
        assertTrue(isMissingFilePlaybackError(caused))
        assertFalse(isMissingFilePlaybackError(other))
    }
}
