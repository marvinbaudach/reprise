package io.github.marvinbaudach.reprise

import kotlinx.coroutines.CancellationException

/**
 * Asks the catalog for the ids behind a whole-album or whole-artist action.
 *
 * The query is unwindowed — a big artist is thousands of rows behind a
 * blocking FFI and SQLite call — so it runs off the main thread, like every
 * other library read (see [offMainLibraryRead]), and the caller resumes on the
 * thread it left. A failure comes back as a [Result] for the caller to say;
 * cancellation does not: a row that left the screen while the query was out
 * has nobody left to tell, and reporting it as a failed load would.
 */
internal suspend fun resolveOffMain(resolve: () -> List<Long>): Result<List<Long>> = try {
    Result.success(readOffMainThread(resolve))
} catch (cancelled: CancellationException) {
    throw cancelled
} catch (error: Throwable) {
    Result.failure(error)
}

internal fun couldNotLoadTracks(error: Throwable): String =
    "Could not load the tracks: ${error.message ?: "unknown error"}"
