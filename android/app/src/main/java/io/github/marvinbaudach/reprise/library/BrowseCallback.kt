package io.github.marvinbaudach.reprise.library

import android.os.Bundle
import android.util.Log
import androidx.media3.common.MediaItem
import androidx.media3.common.util.UnstableApi
import androidx.media3.session.LibraryResult
import androidx.media3.session.MediaLibraryService.LibraryParams
import androidx.media3.session.MediaLibraryService.MediaLibrarySession
import androidx.media3.session.MediaSession
import androidx.media3.session.MediaSession.MediaItemsWithStartPosition
import androidx.media3.session.SessionCommand
import androidx.media3.session.SessionError
import androidx.media3.session.SessionResult
import com.google.common.collect.ImmutableList
import com.google.common.util.concurrent.Futures
import com.google.common.util.concurrent.ListenableFuture
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors

private const val TAG = "RepriseBrowse"

/**
 * Answers a media browser's questions from [tree], on a worker thread: every
 * read is a blocking database query and none may run on the service's main
 * thread. The tree is built on first use, so a service that is never browsed
 * never opens the library on this path.
 *
 * # Who may ask
 *
 * The service is exported, so any installed app can connect. Everything that
 * could reveal or act on the library goes through the single [access] decision,
 * and the answer is refusal by default:
 *
 *  - `onConnect` hands an untrusted controller the plain session commands and
 *    no library commands, so its client-side library calls are not even sent;
 *  - every entry point repeats the check, because the connection-time commands
 *    are advice to the client, not enforcement: root, children, item, search,
 *    search results, subscribe, unsubscribe, custom commands, and the two ways
 *    a browse media id could be turned into a play request (`onAddMediaItems`,
 *    `onSetMediaItems`).
 *
 * Refusals carry only an error code, never text. What an untrusted controller
 * can still see is what the system notification shows: the player's own current
 * item and its transport, which are playback metadata and not browse data.
 */
@androidx.annotation.OptIn(UnstableApi::class)
internal class BrowseCallback(
    private val tree: () -> MediaBrowseTree,
    private val access: BrowserAccess,
    private val executor: ExecutorService = Executors.newSingleThreadExecutor { task ->
        Thread(task, "reprise-browse")
    },
) : MediaLibrarySession.Callback {
    fun close() {
        executor.shutdownNow()
    }

    private fun <T : Any> refused(): ListenableFuture<LibraryResult<T>> =
        Futures.immediateFuture(LibraryResult.ofError(SessionError.ERROR_PERMISSION_DENIED))

    private fun <T : Any> unsupported(): ListenableFuture<LibraryResult<T>> =
        Futures.immediateFuture(LibraryResult.ofError(SessionError.ERROR_NOT_SUPPORTED))

    /**
     * An untrusted controller still connects, and keeps the playback transport,
     * but is told it has no library commands.
     */
    override fun onConnect(
        session: MediaSession,
        controller: MediaSession.ControllerInfo,
    ): MediaSession.ConnectionResult {
        if (access.isAllowed(controller)) return super.onConnect(session, controller)
        return MediaSession.ConnectionResult.AcceptedResultBuilder(session)
            .setAvailableSessionCommands(MediaSession.ConnectionResult.DEFAULT_SESSION_COMMANDS)
            .build()
    }

    override fun onGetLibraryRoot(
        session: MediaLibrarySession,
        browser: MediaSession.ControllerInfo,
        params: LibraryParams?,
    ): ListenableFuture<LibraryResult<MediaItem>> {
        if (!access.isAllowed(browser)) return refused()
        // "Recent" asks for the last played song as a resumable root, which this
        // tree does not model; refusing is the documented way to say so.
        if (params?.isRecent == true) return unsupported()
        return answer { LibraryResult.ofItem(tree().root(), params) }
    }

    override fun onGetItem(
        session: MediaLibrarySession,
        browser: MediaSession.ControllerInfo,
        mediaId: String,
    ): ListenableFuture<LibraryResult<MediaItem>> {
        if (!access.isAllowed(browser)) return refused()
        return answer {
            tree().item(mediaId)?.let { item -> LibraryResult.ofItem(item, null) }
                ?: LibraryResult.ofError(SessionError.ERROR_BAD_VALUE)
        }
    }

    override fun onGetChildren(
        session: MediaLibrarySession,
        browser: MediaSession.ControllerInfo,
        parentId: String,
        page: Int,
        pageSize: Int,
        params: LibraryParams?,
    ): ListenableFuture<LibraryResult<ImmutableList<MediaItem>>> {
        if (!access.isAllowed(browser)) return refused()
        return answer {
            tree().children(parentId, page, pageSize)
                ?.let { items -> LibraryResult.ofItemList(ImmutableList.copyOf(items), params) }
                ?: LibraryResult.ofError(SessionError.ERROR_BAD_VALUE)
        }
    }

    override fun onSearch(
        session: MediaLibrarySession,
        browser: MediaSession.ControllerInfo,
        query: String,
        params: LibraryParams?,
    ): ListenableFuture<LibraryResult<Void>> =
        if (access.isAllowed(browser)) unsupported() else refused()

    override fun onGetSearchResult(
        session: MediaLibrarySession,
        browser: MediaSession.ControllerInfo,
        query: String,
        page: Int,
        pageSize: Int,
        params: LibraryParams?,
    ): ListenableFuture<LibraryResult<ImmutableList<MediaItem>>> =
        if (access.isAllowed(browser)) unsupported() else refused()

    override fun onSubscribe(
        session: MediaLibrarySession,
        browser: MediaSession.ControllerInfo,
        parentId: String,
        params: LibraryParams?,
    ): ListenableFuture<LibraryResult<Void>> =
        if (access.isAllowed(browser)) unsupported() else refused()

    override fun onUnsubscribe(
        session: MediaLibrarySession,
        browser: MediaSession.ControllerInfo,
        parentId: String,
    ): ListenableFuture<LibraryResult<Void>> =
        if (access.isAllowed(browser)) unsupported() else refused()

    /** No custom commands exist; an untrusted controller is told it may not even try. */
    override fun onCustomCommand(
        session: MediaSession,
        controller: MediaSession.ControllerInfo,
        customCommand: SessionCommand,
        args: Bundle,
    ): ListenableFuture<SessionResult> = Futures.immediateFuture(
        SessionResult(
            if (access.isAllowed(controller)) {
                SessionError.ERROR_NOT_SUPPORTED
            } else {
                SessionError.ERROR_PERMISSION_DENIED
            },
        ),
    )

    override fun onAddMediaItems(
        mediaSession: MediaSession,
        controller: MediaSession.ControllerInfo,
        mediaItems: List<MediaItem>,
    ): ListenableFuture<List<MediaItem>> = Futures.submit<List<MediaItem>>(
        { resolve(mediaItems, mayBrowse = access.isAllowed(controller)) },
        executor,
    )

    override fun onSetMediaItems(
        mediaSession: MediaSession,
        controller: MediaSession.ControllerInfo,
        mediaItems: List<MediaItem>,
        startIndex: Int,
        startPositionMs: Long,
    ): ListenableFuture<MediaItemsWithStartPosition> = Futures.submit<MediaItemsWithStartPosition>(
        {
            val mayBrowse = access.isAllowed(controller)
            // One tapped song stands for its whole container, so it is widened
            // to the container here, off the main thread. `BrowsePlayer` then
            // turns the widened list into a Core play request.
            val queue = mediaItems.singleOrNull()
                ?.takeIf { mayBrowse }
                ?.let { item -> tree().queueFor(item.mediaId) }
            if (queue != null) {
                MediaItemsWithStartPosition(
                    queue.trackIds.map { id ->
                        MediaItem.Builder()
                            .setMediaId(BrowseId.Track(queue.container, id).mediaId)
                            .build()
                    },
                    queue.startIndex,
                    startPositionMs,
                )
            } else {
                MediaItemsWithStartPosition(resolve(mediaItems, mayBrowse), startIndex, startPositionMs)
            }
        },
        executor,
    )

    /**
     * Browse items become playable ones; anything else must already carry a uri.
     * A browse media id from a controller that may not browse is refused outright,
     * so the tree cannot be used to resolve or play library items without walking it.
     */
    private fun resolve(mediaItems: List<MediaItem>, mayBrowse: Boolean): List<MediaItem> =
        mediaItems.map { item ->
            if (BrowseId.parse(item.mediaId) == null) {
                item.takeIf { it.localConfiguration != null }
                    ?: throw UnsupportedOperationException("Unknown media item ${item.mediaId}")
            } else {
                if (!mayBrowse) throw SecurityException("This controller may not play library items")
                tree().item(item.mediaId)
                    ?: throw UnsupportedOperationException("Unknown media item ${item.mediaId}")
            }
        }

    private fun <T : Any> answer(read: () -> LibraryResult<T>): ListenableFuture<LibraryResult<T>> =
        Futures.submit<LibraryResult<T>>(
            {
                try {
                    read()
                } catch (error: Exception) {
                    Log.w(TAG, "Could not answer a browse request", error)
                    LibraryResult.ofError(SessionError.ERROR_UNKNOWN)
                }
            },
            executor,
        )
}
