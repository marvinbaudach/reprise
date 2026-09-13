package io.github.marvinbaudach.reprise

import androidx.compose.ui.test.junit4.ComposeTestRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performScrollToIndex
import java.util.concurrent.CountDownLatch
import java.util.concurrent.atomic.AtomicInteger
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.withContext

/**
 * Scrolls to [index], retrying until the row exists. A library window grows through an
 * off-main-thread read, so the row an index names can arrive after the scroll asks for it.
 */
internal fun ComposeTestRule.scrollLibraryListTo(tag: String, index: Int) {
    val node = onNodeWithTag(tag)
    node.assertExists()
    waitUntil(timeoutMillis = 5_000) {
        runCatching { node.performScrollToIndex(index) }.isSuccess
    }
}

internal enum class BlockingReadMode {
    EVERY_CALL,
    FIRST_CALL,
}

internal class BlockingReadTestGate(
    private val mode: BlockingReadMode,
) {
    private val calls = AtomicInteger()
    private var started: CountDownLatch? = null
    private var gate: CompletableDeferred<Unit>? = null
    private var finished: CountDownLatch? = null

    fun arm(resetCallCount: Boolean = false) {
        if (resetCallCount) calls.set(0)
        started = CountDownLatch(1)
        gate = CompletableDeferred()
        finished = CountDownLatch(1)
    }

    fun hasStarted(): Boolean = checkNotNull(started).count == 0L

    fun release() {
        checkNotNull(gate).complete(Unit)
    }

    fun hasFinished(): Boolean = checkNotNull(finished).count == 0L

    fun callCount(): Int = calls.get()

    suspend fun blockCall(): Boolean {
        val callIndex = calls.getAndIncrement()
        if (mode == BlockingReadMode.FIRST_CALL && callIndex != 0) return false
        val started = started ?: return false
        val gate = checkNotNull(gate)
        val finished = checkNotNull(finished)
        started.countDown()
        withContext(NonCancellable) {
            gate.await()
            finished.countDown()
        }
        return true
    }
}
