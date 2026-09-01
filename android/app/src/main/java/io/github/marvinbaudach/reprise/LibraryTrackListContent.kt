package io.github.marvinbaudach.reprise

internal sealed interface TrackListContent {
    data class Row(
        val index: Int,
        val track: LibraryTrack,
    ) : TrackListContent

    data class Continuation(
        val request: LibraryWindowRange,
    ) : TrackListContent
}

internal fun trackListContent(
    window: LibraryWindow<LibraryTrack>,
    lastRequestedOffset: Long?,
): List<TrackListContent> = buildList {
    window.rows.forEachIndexed { index, track ->
        add(TrackListContent.Row(index, track))
    }
    window.nextRequest(lastRequestedOffset)?.let { request ->
        add(TrackListContent.Continuation(request))
    }
}
