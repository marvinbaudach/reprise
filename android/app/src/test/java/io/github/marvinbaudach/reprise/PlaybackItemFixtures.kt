package io.github.marvinbaudach.reprise

import uniffi.reprise_android_ffi.AndroidPlaybackItem
import uniffi.reprise_android_ffi.AndroidPlaybackSegment

/** The item the Core hands the port for [uri]; a whole file unless [segment] cuts it. */
internal fun playbackItem(
    uri: String,
    gainDb: Double = 0.0,
    trackId: Long? = null,
    segment: AndroidPlaybackSegment? = null,
) = AndroidPlaybackItem(trackId = trackId, uri = uri, gainDb = gainDb, segment = segment)
