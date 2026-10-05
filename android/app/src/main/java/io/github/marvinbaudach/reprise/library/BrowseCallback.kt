package io.github.marvinbaudach.reprise.library

import android.os.Bundle
import android.util.Log
import androidx.media3.common.MediaItem
import androidx.media3.common.Player
import androidx.media3.common.util.UnstableApi
import androidx.media3.session.LibraryResult
import androidx.media3.session.MediaLibraryService.LibraryParams
import androidx.media3.session.MediaLibraryService.MediaLibrarySession
import androidx.media3.session.MediaSession
import androidx.media3.session.MediaSession.MediaItemsWithStartPosition
import androidx.media3.session.SessionCommand
import androidx.media3.session.SessionCommands
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
 * could reveal or act on the library or the player goes through the single
 * [access] decision, and the answer is refusal by default:
 *
 *  - `onConnect` hands an untrusted controller no commands at all, neither
 *    session nor player. Media3 filters the player state a controller receives
 *    by its available commands, so such a controller does not even see the
 *    current item, the timeline or the metadata, which name the playing and
 *    the next song; and its client-side calls are not sent;
 *  - an allowed controller (this app, the platform's own surfaces, and a pinned
 *    Android Auto or Wear OS) is given the full default sets, spelled out;
 *  - every entry point repeats the check, because the connection-time commands
 *    are advice to the client, not enforcement: root, children, item, search,
 *    search results, subscribe, unsubscribe, custom commands, every player
 *    command (`onPlayerCommandRequest`), and the two ways a media item could be
 *    turned into a play request (`onAddMediaItems`, `onSetMediaItems`). Only an
 *    allowed controller may bring items at all: a uri-only item would otherwise
 *    play any file, content or network address through ExoPlayer.
 *
 * Refusals carry only an error code, never text.
 *
 * # What is not covered
 *
 *  - [isAllowed][BrowserAccess.isAllowed] is a connect-time snapshot for the
 *    commands (`isTrusted` is fixed when the controller connects); the
 *    entry-point checks above are what a controller that later loses trust
 *    runs into.
 *  - A media button (`ACTION_MEDIA_BUTTON`) is not routed through this
 *    callback: Media3 hands it to the session as the notification controller,
 *    or, from the platform session, as a controller that is never marked
 *    trusted, whoever sent it. A headset or the system cannot be told from an
 *    app that fires the intent, so refusing would break the headset. It reaches
 *    play, pause and skip only, which is the accepted residual.
 *  - `onGetSession` receives only a placeholder controller (the legacy-service
 *    bind and the media-button fallback carry no caller identity), so it cannot
 *    refuse a caller; the per-controller decision is made in `onConnect`.
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
     * An untrusted controller still connects, but with nothing: no session
     * command and no player command. Media3 filters the state it sends by these,
     * so the current item and the timeline stay private too.
     */
    override fun onConnect(
        session: MediaSession,
        controller: MediaSession.ControllerInfo,
    ): MediaSession.ConnectionResult =
        if (access.isAllowed(controller)) {
            MediaSession.ConnectionResult.accept(
                MediaSession.ConnectionResult.DEFAULT_SESSION_AND_LIBRARY_COMMANDS,
                MediaSession.ConnectionResult.DEFAULT_PLAYER_COMMANDS,
            )
        } else {
            MediaSession.ConnectionResult.accept(SessionCommands.EMPTY, Player.Commands.EMPTY)
        }

    /**
     * The connect-time commands are advice; this is the enforcement for every
     * player call. Media3 1.11 marks the method deprecated without a successor
     * that sees the calling controller, and it is still what the session asks.
     */
    @Suppress("OVERRIDE_DEPRECATION")
    override fun onPlayerCommandRequest(
        session: MediaSession,
        controller: MediaSession.ControllerInfo,
        playerCommand: Int,
    ): Int = if (access.isAllowed(controller)) {
        SessionResult.RESULT_SUCCESS
    } else {
        SessionError.ERROR_PERMISSION_DENIED
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
     * A controller that may not browse brings no item at all, browse id or uri:
     * the former would resolve library items without walking the tree, the
     * latter would hand ExoPlayer an address of the caller's choosing.
     */
    private fun resolve(mediaItems: List<MediaItem>, mayBrowse: Boolean): List<MediaItem> {
        if (!mayBrowse) throw SecurityException("This controller may not play media items")
        return mediaItems.map { item ->
            if (BrowseId.parse(item.mediaId) == null) {
                item.takeIf { it.localConfiguration != null }
                    ?: throw UnsupportedOperationException("Unknown media item ${item.mediaId}")
            } else {
                tree().item(item.mediaId)
                    ?: throw UnsupportedOperationException("Unknown media item ${item.mediaId}")
            }
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
