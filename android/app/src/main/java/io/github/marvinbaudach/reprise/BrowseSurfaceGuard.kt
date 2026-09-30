package io.github.marvinbaudach.reprise

import androidx.compose.runtime.MutableState
import androidx.compose.runtime.State
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue

/**
 * Whether a read that has come back still describes what is on screen, and the
 * one error line the screen keeps for the read that last failed.
 *
 * The state lives in [BrowseScreen], keyed there by what invalidates it; this
 * only reads and writes through the [State] it was handed, so a value asked for
 * here is the one on screen at the moment of asking.
 */
internal class BrowseSurfaceGuard(
    private val surfaceState: MobileSurfaceViewModel,
    pendingAlbumState: State<LibraryAlbum?>,
    pendingArtistState: State<LibraryArtist?>,
    selectedAlbumState: State<AlbumTrackList?>,
    selectedArtistState: State<ArtistTrackList?>,
    browseErrorState: MutableState<String?>,
    browseErrorOriginState: MutableState<BrowseErrorOrigin?>,
) {
    private val pendingAlbum by pendingAlbumState
    private val pendingArtist by pendingArtistState
    private val selectedAlbum by selectedAlbumState
    private val selectedArtist by selectedArtistState
    private var browseError by browseErrorState
    private var browseErrorOrigin by browseErrorOriginState

    fun tabSurfaceIsCurrent(tab: BrowseTab): Boolean =
        surfaceState.selectedTab == tab && pendingAlbum == null && pendingArtist == null &&
            selectedAlbum == null && selectedArtist == null

    fun tabQueryResultIsCurrent(tab: BrowseTab): Boolean =
        surfaceState.selectedTab == tab && selectedAlbum == null && selectedArtist == null

    fun artistSurfaceIsCurrent(artist: LibraryArtist?): Boolean =
        surfaceState.selectedTab == BrowseTab.ARTISTS &&
            pendingAlbum == null && pendingArtist == null &&
            selectedArtist?.artist == artist && selectedAlbum == null

    fun albumOpenIsCurrent(album: LibraryAlbum, parentArtist: LibraryArtist?): Boolean =
        surfaceState.selectedTab == BrowseTab.ARTISTS && pendingAlbum == album &&
            pendingArtist == null && selectedArtist?.artist == parentArtist

    fun artistOpenIsCurrent(artist: LibraryArtist): Boolean =
        surfaceState.selectedTab == BrowseTab.ARTISTS && pendingAlbum == null &&
            selectedAlbum == null &&
            ((pendingArtist == artist && selectedArtist == null) ||
                (pendingArtist == null && selectedArtist?.artist == artist))

    fun albumSurfaceIsCurrent(album: LibraryAlbum): Boolean =
        surfaceState.selectedTab == BrowseTab.ARTISTS && selectedAlbum?.album == album

    fun errorOriginIsCurrent(origin: BrowseErrorOrigin): Boolean = when (origin) {
        is BrowseErrorOrigin.Tab -> tabSurfaceIsCurrent(origin.tab)
        is BrowseErrorOrigin.Artist -> artistSurfaceIsCurrent(origin.artist)
        is BrowseErrorOrigin.Album -> albumSurfaceIsCurrent(origin.album)
    }

    fun setBrowseError(message: String, origin: BrowseErrorOrigin) {
        browseError = message
        browseErrorOrigin = origin
    }

    fun clearBrowseError(origin: BrowseErrorOrigin) {
        if (browseErrorOrigin == null || browseErrorOrigin == origin) {
            browseError = null
            browseErrorOrigin = null
        }
    }
}
