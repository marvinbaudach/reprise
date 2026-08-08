package de.reprise.spike

import java.io.IOException
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ListenReportWriterTest {
    @Test
    fun missingAcknowledgementMeansNothingWasAcknowledgedAndTheReportIsStillWritten() {
        var receivedAcknowledgement: ByteArray? = byteArrayOf(99)
        var written: ByteArray? = null
        val writer = ListenReportWriter(
            readAcknowledgement = { null },
            produceReport = { acknowledgement ->
                receivedAcknowledgement = acknowledgement
                byteArrayOf(1, 2, 3)
            },
            writeReport = { bytes -> written = bytes },
        )

        assertTrue(writer.publish().isSuccess)
        assertNull(receivedAcknowledgement)
        assertArrayEquals(byteArrayOf(1, 2, 3), written)
    }

    @Test
    fun unreadableAcknowledgementAlsoMeansNothingWasAcknowledged() {
        var receivedAcknowledgement: ByteArray? = byteArrayOf(99)
        var writes = 0
        val writer = ListenReportWriter(
            readAcknowledgement = { throw IOException("truncated provider read") },
            produceReport = { acknowledgement ->
                receivedAcknowledgement = acknowledgement
                byteArrayOf(4, 5, 6)
            },
            writeReport = { writes++ },
        )

        assertTrue(writer.publish().isSuccess)
        assertNull(receivedAcknowledgement)
        assertEquals(1, writes)
    }
}
