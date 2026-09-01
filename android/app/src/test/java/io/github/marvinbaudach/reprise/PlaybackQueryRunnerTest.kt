package io.github.marvinbaudach.reprise

import java.util.concurrent.CountDownLatch
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

private const val QUERY_WAIT_SECONDS = 5L

class PlaybackQueryRunnerTest {
    @Test
    fun queueBoundaryCallsRunAndAnswerInSubmissionOrder() {
        val firstStarted = CountDownLatch(1)
        val releaseFirst = CountDownLatch(1)
        val secondStarted = CountDownLatch(1)
        val answers = LinkedBlockingQueue<Int>()
        val runner = PlaybackQueryRunner()

        try {
            runner.query(
                operation = {
                    firstStarted.countDown()
                    assertTrue(releaseFirst.await(QUERY_WAIT_SECONDS, TimeUnit.SECONDS))
                    1
                },
                report = { outcome -> answers.put(outcome.getOrThrow()) },
            )
            assertTrue(firstStarted.await(QUERY_WAIT_SECONDS, TimeUnit.SECONDS))

            runner.query(
                operation = {
                    secondStarted.countDown()
                    2
                },
                report = { outcome -> answers.put(outcome.getOrThrow()) },
            )
            assertFalse(
                "a later queue edit must wait for the earlier one",
                secondStarted.await(200, TimeUnit.MILLISECONDS),
            )

            releaseFirst.countDown()
            assertTrue(secondStarted.await(QUERY_WAIT_SECONDS, TimeUnit.SECONDS))
            assertEquals(1, answers.poll(QUERY_WAIT_SECONDS, TimeUnit.SECONDS))
            assertEquals(2, answers.poll(QUERY_WAIT_SECONDS, TimeUnit.SECONDS))
        } finally {
            releaseFirst.countDown()
            runner.shutdown()
        }
    }
}
