package io.github.marvinbaudach.reprise

/**
 * One answered request for the playing track's row, carrying the id it was
 * asked for. The id says whether the retained row still describes the track
 * the session reports as playing, so actions can be disabled during a change.
 */
internal data class AnsweredTrack(val id: Long, val track: LibraryTrack?)

/**
 * One tab's freshly fetched windows, carried out of the IO dispatcher.
 *
 * A tab fills a different set of windows than its neighbours, and a window it
 * does not fill is `null` rather than empty: an empty window is a real answer
 * — "no artists match this" — and assigning one where nothing was asked for
 * would blank a list that had rows.
 */
internal data class LoadedTab(
    val titles: LibraryWindow<LibraryTrack>? = null,
    val artists: LibraryWindow<LibraryArtist>? = null,
)
