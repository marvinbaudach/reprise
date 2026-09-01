package io.github.marvinbaudach.reprise

import android.util.Log
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

private const val RESTORE_TAG = "RepriseScan"

internal fun CoroutineScope.launchLibraryRestore(
    dispatcher: CoroutineDispatcher,
    restore: () -> LibraryScreenState,
    report: (LibraryScreenState) -> Unit,
): Job = launch {
    report(withContext(dispatcher) { restore() })
}

internal fun restoreLibraryState(
    session: LibrarySession,
    selectedTab: BrowseTab,
): LibraryScreenState = runCatching {
    session.restore(selectedTab)
}.getOrElse { error ->
    val detail = error.message ?: error.javaClass.simpleName
    val message = "Could not load the saved library: $detail"
    Log.e(RESTORE_TAG, message, error)
    runCatching { session.stateAfterFailure(message) }
        .getOrDefault(LibraryScreenState.NoFolder(message))
}
