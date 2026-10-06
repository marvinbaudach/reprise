package io.github.marvinbaudach.reprise

import androidx.compose.runtime.MutableState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import kotlinx.coroutines.CancellationException

/**
 * The continuation reads of the browse lists: the next window of titles,
 * artists, an album's tracks, and an artist's albums and other titles.
 *
 * Like [BrowseSurfaceGuard] this holds no state of its own. [BrowseScreen] owns
 * the windows and the offsets already requested, keyed by what resets them, and
 * hands them over as [MutableState]; a read here writes its answer back into
 * them. It is built afresh on every composition so that [searchText] and the
 * readers are the ones the screen has now.
 */
internal class BrowsePaging(
    private val guard: BrowseSurfaceGuard,
    private val readJobs: BrowseReadJobs,
    private val loadsInFlight: MutableSet<String>,
    private val searchText: String,
    visibleTitlesState: MutableState<LibraryWindow<LibraryTrack>>,
    visibleArtistsState: MutableState<LibraryWindow<LibraryArtist>>,
    selectedAlbumState: MutableState<AlbumTrackList?>,
    selectedArtistState: MutableState<ArtistTrackList?>,
    titlesRequestedOffsetState: MutableState<Long?>,
    artistsRequestedOffsetState: MutableState<Long?>,
    albumRequestedOffsetState: MutableState<Long?>,
    artistRequestedOffsetState: MutableState<Long?>,
    artistAlbumsRequestedOffsetState: MutableState<Long?>,
    private val searchTitles: suspend (String, LibraryWindowRange) -> LibraryWindow<LibraryTrack>,
    private val artistsFor: suspend (String, LibraryWindowRange) -> LibraryWindow<LibraryArtist>,
    private val listAlbumTracks:
        suspend (LibraryAlbum, LibraryWindowRange) -> LibraryWindow<LibraryTrack>,
    private val listArtistUntaggedTracks:
        suspend (LibraryArtist, LibraryWindowRange) -> LibraryWindow<LibraryTrack>,
    private val listArtistAlbums:
        suspend (LibraryArtist, LibraryWindowRange) -> LibraryWindow<LibraryAlbum>,
) {
    private var visibleTitles by visibleTitlesState
    private var visibleArtists by visibleArtistsState
    private var selectedAlbum by selectedAlbumState
    private var selectedArtist by selectedArtistState
    private var titlesRequestedOffset by titlesRequestedOffsetState
    private var artistsRequestedOffset by artistsRequestedOffsetState
    private var albumRequestedOffset by albumRequestedOffsetState
    private var artistRequestedOffset by artistRequestedOffsetState
    private var artistAlbumsRequestedOffset by artistAlbumsRequestedOffsetState

    // Runs `body` for `key` unless a read for that same key is already in
    // flight, and always clears the key afterwards — including when `body`
    // is cancelled, which is exactly why this is try/finally rather than a
    // clear-on-success inside `onSuccess`.
    private suspend fun guardedAgainstDuplicateLoad(key: String, body: suspend () -> Unit) {
        if (!loadsInFlight.add(key)) return
        try {
            body()
        } finally {
            loadsInFlight.remove(key)
        }
    }

    suspend fun loadMoreTitles(request: LibraryWindowRange) {
        if (visibleTitles.nextRequest(titlesRequestedOffset) != request) return
        guardedAgainstDuplicateLoad("titles:${request.offset}") {
            runCatching { searchTitles(searchText, request) }
                .onSuccess { continuation ->
                    titlesRequestedOffset = request.offset
                    visibleTitles = visibleTitles.append(continuation)
                    if (guard.tabSurfaceIsCurrent(BrowseTab.TITLES)) {
                        guard.clearBrowseError(BrowseErrorOrigin.Tab(BrowseTab.TITLES))
                    }
                }
                .onFailure { error ->
                    if (error is CancellationException) throw error
                    if (guard.tabSurfaceIsCurrent(BrowseTab.TITLES)) {
                        guard.setBrowseError(
                            error.browseDetail("load more titles"),
                            BrowseErrorOrigin.Tab(BrowseTab.TITLES),
                        )
                    }
                }
        }
    }

    suspend fun loadMoreArtists(request: LibraryWindowRange) {
        if (visibleArtists.nextRequest(artistsRequestedOffset) != request) return
        guardedAgainstDuplicateLoad("artists:${request.offset}") {
            runCatching { artistsFor(searchText, request) }
                .onSuccess { continuation ->
                    artistsRequestedOffset = request.offset
                    visibleArtists = visibleArtists.append(continuation)
                    if (guard.tabSurfaceIsCurrent(BrowseTab.ARTISTS)) {
                        guard.clearBrowseError(BrowseErrorOrigin.Tab(BrowseTab.ARTISTS))
                    }
                }
                .onFailure { error ->
                    if (error is CancellationException) throw error
                    if (guard.tabSurfaceIsCurrent(BrowseTab.ARTISTS)) {
                        guard.setBrowseError(
                            error.browseDetail("load more artists"),
                            BrowseErrorOrigin.Tab(BrowseTab.ARTISTS),
                        )
                    }
                }
        }
    }

    suspend fun loadMoreAlbumTracks(request: LibraryWindowRange) {
        val detail = selectedAlbum ?: return
        if (detail.tracks.nextRequest(albumRequestedOffset) != request) return
        guardedAgainstDuplicateLoad("album-tracks:${request.offset}") {
            runCatching { listAlbumTracks(detail.album, request) }
                .onSuccess { continuation ->
                    albumRequestedOffset = request.offset
                    selectedAlbum = detail.copy(tracks = detail.tracks.append(continuation))
                    if (guard.albumSurfaceIsCurrent(detail.album)) {
                        guard.clearBrowseError(BrowseErrorOrigin.Album(detail.album))
                    }
                }
                .onFailure { error ->
                    if (error is CancellationException) throw error
                    if (guard.albumSurfaceIsCurrent(detail.album)) {
                        guard.setBrowseError(
                            error.browseDetail("load more album tracks"),
                            BrowseErrorOrigin.Album(detail.album),
                        )
                    }
                }
        }
    }

    suspend fun loadMoreArtistTracks(request: LibraryWindowRange) {
        val detail = selectedArtist ?: return
        val artistOpenRequest = readJobs.latestArtistOpen
        if (detail.untaggedTracks.nextRequest(artistRequestedOffset) != request) return
        guardedAgainstDuplicateLoad("artist-tracks:${request.offset}") {
            runCatching { listArtistUntaggedTracks(detail.artist, request) }
                .onSuccess { continuation ->
                    if (
                        artistOpenRequest != readJobs.latestArtistOpen ||
                        !guard.artistOpenIsCurrent(detail.artist)
                    ) return@onSuccess
                    artistRequestedOffset = request.offset
                    selectedArtist = detail.copy(
                        untaggedTracks = detail.untaggedTracks.append(continuation),
                    )
                    if (guard.artistSurfaceIsCurrent(detail.artist)) {
                        guard.clearBrowseError(BrowseErrorOrigin.Artist(detail.artist))
                    }
                }
                .onFailure { error ->
                    if (error is CancellationException) throw error
                    if (guard.artistSurfaceIsCurrent(detail.artist)) {
                        guard.setBrowseError(
                            error.browseDetail("load more other titles"),
                            BrowseErrorOrigin.Artist(detail.artist),
                        )
                    }
                }
        }
    }

    suspend fun loadMoreArtistAlbums(request: LibraryWindowRange) {
        val detail = selectedArtist ?: return
        val artistOpenRequest = readJobs.latestArtistOpen
        if (detail.albums.nextRequest(artistAlbumsRequestedOffset) != request) return
        guardedAgainstDuplicateLoad("artist-albums:${request.offset}") {
            runCatching { listArtistAlbums(detail.artist, request) }
                .onSuccess { continuation ->
                    if (
                        artistOpenRequest != readJobs.latestArtistOpen ||
                        !guard.artistOpenIsCurrent(detail.artist)
                    ) return@onSuccess
                    artistAlbumsRequestedOffset = request.offset
                    selectedArtist = detail.copy(albums = detail.albums.append(continuation))
                    if (guard.artistSurfaceIsCurrent(detail.artist)) {
                        guard.clearBrowseError(BrowseErrorOrigin.Artist(detail.artist))
                    }
                }
                .onFailure { error ->
                    if (error is CancellationException) throw error
                    if (guard.artistSurfaceIsCurrent(detail.artist)) {
                        guard.setBrowseError(
                            error.browseDetail("load more artist albums"),
                            BrowseErrorOrigin.Artist(detail.artist),
                        )
                    }
                }
        }
    }
}
