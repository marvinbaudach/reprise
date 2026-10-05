package io.github.marvinbaudach.reprise.library

import androidx.media3.common.ForwardingPlayer
import androidx.media3.common.MediaItem
import androidx.media3.common.Player
import androidx.media3.common.util.UnstableApi

/**
 * Keeps media-browser play requests inside the Core's queue.
 *
 * When Android Auto starts a song, Media3 hands the session player the browse
 * items to play. Passed on, ExoPlayer would play them itself and the Core —
 * which owns the queue, the history and the play counts — would never hear of
 * it. So an item that comes from the browse tree is turned into a Core play
 * request instead, and everything else is forwarded untouched, which keeps the
 * app's own session behaviour as it was.
 */
// ForwardingPlayer's overrides are unstable in media3 1.11; one opt-in covers
// the class instead of a baseline entry per override.
@androidx.annotation.OptIn(UnstableApi::class)
internal class BrowsePlayer(
    player: Player,
    private val playQueue: (trackIds: List<Long>, startIndex: Int) -> Unit,
) : ForwardingPlayer(player) {
    override fun setMediaItem(mediaItem: MediaItem) {
        if (!intercept(listOf(mediaItem), 0)) super.setMediaItem(mediaItem)
    }

    override fun setMediaItem(mediaItem: MediaItem, startPositionMs: Long) {
        if (!intercept(listOf(mediaItem), 0)) super.setMediaItem(mediaItem, startPositionMs)
    }

    override fun setMediaItem(mediaItem: MediaItem, resetPosition: Boolean) {
        if (!intercept(listOf(mediaItem), 0)) super.setMediaItem(mediaItem, resetPosition)
    }

    override fun setMediaItems(mediaItems: List<MediaItem>) {
        if (!intercept(mediaItems, 0)) super.setMediaItems(mediaItems)
    }

    override fun setMediaItems(mediaItems: List<MediaItem>, resetPosition: Boolean) {
        if (!intercept(mediaItems, 0)) super.setMediaItems(mediaItems, resetPosition)
    }

    override fun setMediaItems(mediaItems: List<MediaItem>, startIndex: Int, startPositionMs: Long) {
        if (!intercept(mediaItems, startIndex)) {
            super.setMediaItems(mediaItems, startIndex, startPositionMs)
        }
    }

    // A browser appending a song to "the queue" would reach ExoPlayer's list,
    // not the Core's, so these are swallowed rather than forwarded.
    override fun addMediaItem(mediaItem: MediaItem) {
        if (!isBrowseItem(mediaItem)) super.addMediaItem(mediaItem)
    }

    override fun addMediaItem(index: Int, mediaItem: MediaItem) {
        if (!isBrowseItem(mediaItem)) super.addMediaItem(index, mediaItem)
    }

    override fun addMediaItems(mediaItems: List<MediaItem>) {
        if (!mediaItems.any(::isBrowseItem)) super.addMediaItems(mediaItems)
    }

    override fun addMediaItems(index: Int, mediaItems: List<MediaItem>) {
        if (!mediaItems.any(::isBrowseItem)) super.addMediaItems(index, mediaItems)
    }

    /** `true` when [mediaItems] were browse songs and have been played through the Core. */
    private fun intercept(mediaItems: List<MediaItem>, startIndex: Int): Boolean {
        if (mediaItems.isEmpty()) return false
        val trackIds = mediaItems.map { item ->
            (BrowseId.parse(item.mediaId) as? BrowseId.Track)?.trackId ?: return false
        }
        playQueue(trackIds, startIndex.coerceIn(0, trackIds.lastIndex))
        return true
    }

    private fun isBrowseItem(mediaItem: MediaItem): Boolean =
        BrowseId.parse(mediaItem.mediaId) is BrowseId.Track
}
