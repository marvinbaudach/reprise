package io.github.marvinbaudach.reprise.widget

import io.github.marvinbaudach.reprise.library.TrackMetadata
import uniffi.reprise_android_ffi.AndroidPlaybackSnapshot
import uniffi.reprise_android_ffi.AndroidPlaybackState
import uniffi.reprise_android_ffi.AndroidRepeatMode

internal fun snapshot(
    state: AndroidPlaybackState,
    trackId: Long?,
    positionMs: Long = 0,
) = AndroidPlaybackSnapshot(
    state = state,
    currentIndex = trackId?.let { 0UL },
    currentTrackId = trackId,
    currentTrackUri = trackId?.let { "content://tree/$it.flac" },
    positionMs = positionMs,
    durationMs = 0,
    automaticAdvanceCount = 0u,
    shuffled = false,
    repeat = AndroidRepeatMode.OFF,
    error = null,
)

internal fun metadataFor(uri: String): TrackMetadata? {
    val id = uri.substringAfterLast('/').substringBefore('.').toLongOrNull() ?: return null
    return TrackMetadata(id, "Title $id", "Artist $id", "Album", 1_000)
}
