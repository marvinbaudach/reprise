package io.github.marvinbaudach.reprise

import java.util.concurrent.CountDownLatch
import java.util.concurrent.atomic.AtomicReference

/** Runs Media3 commands inline on its Looper and synchronously hands off all others. */
internal class ApplicationLooperDispatch(
    private val isApplicationThread: () -> Boolean,
    private val post: ((() -> Unit) -> Boolean),
) {
    fun <T> call(command: () -> T): T {
        if (isApplicationThread()) {
            return command()
        }

        val completion = CountDownLatch(1)
        val outcome = AtomicReference<Result<T>>()
        check(
            post {
                outcome.set(runCatching(command))
                completion.countDown()
            },
        ) { "Media3 application Looper rejected a playback command" }
        completion.await()
        return checkNotNull(outcome.get()).getOrThrow()
    }
}
