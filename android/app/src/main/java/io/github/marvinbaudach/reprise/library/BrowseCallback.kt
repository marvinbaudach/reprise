package io.github.marvinbaudach.reprise.library

import android.util.Log
import androidx.media3.common.MediaItem
import androidx.media3.common.util.UnstableApi
import androidx.media3.session.LibraryResult
import androidx.media3.session.MediaLibraryService.LibraryParams
import androidx.media3.session.MediaLibraryService.MediaLibrarySession
import androidx.media3.session.MediaSession
import androidx.media3.session.MediaSession.MediaItemsWithStartPosition
import androidx.media3.session.SessionError
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
 */
@androidx.annotation.OptIn(UnstableApi::class)
internal class BrowseCallback(
    private val tree: () -> MediaBrowseTree,
    private val ownPackage: String,
    private val trust: (packageName: String, platformTrusted: Boolean, ownPackage: String) -> Boolean =
        ::isTrustedBrowser,
    private val executor: ExecutorService = Executors.newSingleThreadExecutor { task ->
        Thread(task, "reprise-browse")
    },
) : MediaLibrarySession.Callback {
    fun close() {
        executor.shutdownNow()
    }

    private fun MediaSession.ControllerInfo.mayBrowse(): Boolean =
        trust(packageName, isTrusted, ownPackage)

    private fun <T : Any> denied(): ListenableFuture<LibraryResult<T>> =
        Futures.immediateFuture(LibraryResult.ofError(SessionError.ERROR_PERMISSION_DENIED))

    /**
     * An untrusted controller still connects, and keeps the playback transport,
     * but without the library commands: it cannot ask for the root, a folder or
     * an item. Each handler below checks again, so a command that slips through
     * is refused rather than answered.
     */
    override fun onConnect(
        session: MediaSession,
        controller: MediaSession.ControllerInfo,
    ): MediaSession.ConnectionResult {
        if (controller.mayBrowse()) return super.onConnect(session, controller)
        return MediaSession.ConnectionResult.AcceptedResultBuilder(session)
            .setAvailableSessionCommands(MediaSession.ConnectionResult.DEFAULT_SESSION_COMMANDS)
            .build()
    }

    override fun onGetLibraryRoot(
        session: MediaLibrarySession,
        browser: MediaSession.ControllerInfo,
        params: LibraryParams?,
    ): ListenableFuture<LibraryResult<MediaItem>> {
        if (!browser.mayBrowse()) return denied()
        // "Recent" asks for the last played song as a resumable root, which this
        // tree does not model; refusing is the documented way to say so.
        if (params?.isRecent == true) {
            return Futures.immediateFuture(LibraryResult.ofError(SessionError.ERROR_NOT_SUPPORTED))
        }
        return answer { LibraryResult.ofItem(tree().root(), params) }
    }

    override fun onGetItem(
        session: MediaLibrarySession,
        browser: MediaSession.ControllerInfo,
        mediaId: String,
    ): ListenableFuture<LibraryResult<MediaItem>> = if (!browser.mayBrowse()) denied() else answer {
        tree().item(mediaId)?.let { item -> LibraryResult.ofItem(item, null) }
            ?: LibraryResult.ofError(SessionError.ERROR_BAD_VALUE)
    }

    override fun onGetChildren(
        session: MediaLibrarySession,
        browser: MediaSession.ControllerInfo,
        parentId: String,
        page: Int,
        pageSize: Int,
        params: LibraryParams?,
    ): ListenableFuture<LibraryResult<ImmutableList<MediaItem>>> = if (!browser.mayBrowse()) denied() else answer {
        tree().children(parentId, page, pageSize)
            ?.let { items -> LibraryResult.ofItemList(ImmutableList.copyOf(items), params) }
            ?: LibraryResult.ofError(SessionError.ERROR_BAD_VALUE)
    }

    override fun onAddMediaItems(
        mediaSession: MediaSession,
        controller: MediaSession.ControllerInfo,
        mediaItems: List<MediaItem>,
    ): ListenableFuture<List<MediaItem>> = Futures.submit<List<MediaItem>>(
        { resolve(mediaItems, mayBrowse = controller.mayBrowse()) },
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
            val mayBrowse = controller.mayBrowse()
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

    /** Browse items become playable ones; anything else must already carry a uri. */
    private fun resolve(mediaItems: List<MediaItem>, mayBrowse: Boolean): List<MediaItem> = mediaItems.map { item ->
        if (BrowseId.parse(item.mediaId) != null && !mayBrowse) {
            throw SecurityException("This controller may not play library items")
        }
        tree().item(item.mediaId)
            ?: item.takeIf { it.localConfiguration != null }
            ?: throw UnsupportedOperationException("Unknown media item ${item.mediaId}")
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
