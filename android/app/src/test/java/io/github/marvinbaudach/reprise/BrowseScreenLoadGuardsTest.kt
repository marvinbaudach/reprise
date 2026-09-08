package io.github.marvinbaudach.reprise

import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performScrollToIndex
import java.util.concurrent.CountDownLatch
import java.util.concurrent.atomic.AtomicInteger
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.withContext
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

/**
 * Two `BrowseScreen` behaviours the off-main-thread landing left uncovered:
 * the guard that keeps a relaunched pagination sentinel from firing a second
 * identical read, and the cancellation path that must not let an abandoned
 * read surface as a browse error.
 *
 * Both gate the titles continuation directly, and both wrap the gate wait in
 * `NonCancellable` — that is what a blocking JNI/SQLite read would do:
 * ignore the cancellation of whatever composable asked for it and keep
 * running until it is actually done. Cancelling the *asking* composable
 * without that would just interrupt the fake's own wait, which proves
 * nothing about either guard.
 */
@RunWith(RobolectricTestRunner::class)
@Config(
    sdk = [36],
    qualifiers = "w412dp-h916dp-port",
    application = BrowseScreenLoadGuardsTestApplication::class,
)
class BrowseScreenLoadGuardsTest {
    @get:Rule
    val compose = createAndroidComposeRule<MainActivity>()

    private val application: BrowseScreenLoadGuardsTestApplication
        get() = RuntimeEnvironment.getApplication() as BrowseScreenLoadGuardsTestApplication

    @After
    fun releaseTheService() {
        application.releaseService()
    }

    /**
     * Scrolling the sentinel row far away and back disposes and recomposes
     * it, so its `LaunchedEffect(request.offset)` fires a second time with
     * the same key while the first read is still working.
     * `guardedAgainstDuplicateLoad`'s `if (!loadsInFlight.add(key)) return`
     * is the only thing standing between that relaunch and a second call to
     * `searchTitles` for the same offset; delete it and the single call
     * recorded below becomes two.
     *
     * The second, unguarded cycle at the end is what keeps the first
     * assertion honest: it proves the relaunch itself really happens, so
     * `assertEquals(1, …)` cannot be passing merely because the sentinel
     * never came back to life.
     */
    @Test
    fun aRelaunchedSentinelDoesNotDuplicateTheStillRunningRead() {
        application.blockTitleContinuation()
        scrollLibraryListTo("library-titles-list", 200)
        compose.waitUntil(timeoutMillis = 5_000) { application.continuationHasStarted() }

        // Far enough that the sentinel row's own composition is disposed,
        // then back to the same offset: the row that comes back is a fresh
        // composition, and its LaunchedEffect fires again with the same key
        // while the first read is still gated below.
        scrollLibraryListTo("library-titles-list", 0)
        compose.waitForIdle()
        scrollLibraryListTo("library-titles-list", 200)
        compose.waitForIdle()

        application.releaseTitleContinuation()
        compose.waitUntil(timeoutMillis = 5_000) { application.continuationHasFinished() }
        compose.waitForIdle()

        assertEquals(1, application.continuationCallCount())

        // Nothing gates this second cycle against the first: it is a plain
        // repeat of scroll-away, scroll-back, release, and it really does
        // fetch the next window once nothing is left in flight for it to
        // collide with.
        application.blockTitleContinuation()
        scrollLibraryListTo("library-titles-list", 0)
        compose.waitForIdle()
        scrollLibraryListTo("library-titles-list", 200)
        compose.waitUntil(timeoutMillis = 5_000) { application.continuationHasStarted() }
        application.releaseTitleContinuation()
        compose.waitUntil(timeoutMillis = 5_000) { application.continuationHasFinished() }
        compose.waitForIdle()

        assertEquals(2, application.continuationCallCount())
        compose.onNodeWithText("400 of 450 titles loaded").assertExists()
    }

    /**
     * `loadMoreTitles`'s `.onFailure` rethrows a `CancellationException`
     * before it can become a browse error
     * (`if (error is CancellationException) throw error`). Deleting that
     * line turns the abandoned read below into a rendered
     * "Could not load more titles:" banner.
     *
     * The sentinel is disposed by scrolling away without scrolling back, and
     * the Titles tab itself is never left — so `tabSurfaceIsCurrent` stays
     * true throughout and cannot be what is hiding the banner; the rethrow
     * has to be.
     */
    @Test
    fun anAbandonedReadShowsNoBrowseError() {
        application.blockTitleContinuation()
        scrollLibraryListTo("library-titles-list", 200)
        compose.waitUntil(timeoutMillis = 5_000) { application.continuationHasStarted() }

        // Disposes the sentinel's composition and cancels its
        // LaunchedEffect while the read is still gated below.
        scrollLibraryListTo("library-titles-list", 0)
        compose.waitForIdle()

        application.releaseTitleContinuation()
        compose.waitUntil(timeoutMillis = 5_000) { application.continuationHasFinished() }
        compose.waitForIdle()

        // Genuinely abandoned, not quietly completed: the window is still
        // the first 200 rows.
        compose.onNodeWithText("200 of 450 titles loaded").assertExists()
        compose.onNodeWithText("Could not load more titles:", substring = true)
            .assertDoesNotExist()
    }

    private fun scrollLibraryListTo(tag: String, index: Int) {
        val node = compose.onNodeWithTag(tag)
        node.assertExists()
        compose.waitUntil(timeoutMillis = 5_000) {
            runCatching { node.performScrollToIndex(index) }.isSuccess
        }
    }
}

internal class BrowseScreenLoadGuardsTestApplication : ConfigurationTestApplication() {
    private val calls = AtomicInteger()
    private var started: CountDownLatch? = null
    private var gate: CompletableDeferred<Unit>? = null
    private var finished: CountDownLatch? = null

    fun blockTitleContinuation() {
        started = CountDownLatch(1)
        gate = CompletableDeferred()
        finished = CountDownLatch(1)
    }

    fun continuationHasStarted(): Boolean = checkNotNull(started).count == 0L

    fun releaseTitleContinuation() {
        checkNotNull(gate).complete(Unit)
    }

    fun continuationHasFinished(): Boolean = checkNotNull(finished).count == 0L

    fun continuationCallCount(): Int = calls.get()

    override fun mainActivitySurface(): MainActivitySurfaceDependencies {
        val dependencies = super.mainActivitySurface()
        return dependencies.copy(
            searchTitles = { query, range ->
                if (range.offset > 0) {
                    calls.incrementAndGet()
                    started?.countDown()
                    // NonCancellable so disposing the composable that asked
                    // for this — the sentinel row, once scrolled away — does
                    // not interrupt the wait, the same way a blocking
                    // JNI/SQLite read could not be interrupted either. The
                    // finished countdown lives inside this block because
                    // resuming past it can throw once the caller is
                    // cancelled, which would otherwise leave it uncounted.
                    withContext(NonCancellable) {
                        gate?.await()
                        finished?.countDown()
                    }
                }
                dependencies.searchTitles(query, range)
            },
        )
    }
}
