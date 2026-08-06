package de.reprise.spike

import android.net.Uri

/** A JVM-replaceable library edge; activity, service, ViewModel and UI stay real. */
internal interface MainActivitySurfaceProvider {
    fun mainActivitySurface(): MainActivitySurfaceDependencies
}

internal data class MainActivitySurfaceDependencies(
    val initialTheme: MobileThemeSelection,
    val initialVisualizer: MobileVisualizer,
    val initialState: LibraryScreenState,
    val artwork: () -> TrackArtwork?,
    val playbackControls: PlaybackControls,
    val chooseFolder: (Uri, (LibraryScreenState) -> Unit) -> Unit,
    val rescan: ((LibraryScreenState) -> Unit) -> Unit,
    val searchTitles: (String, LibraryWindowRange) -> LibraryWindow<LibraryTrack>,
    val listAlbums: (LibraryWindowRange) -> LibraryWindow<LibraryAlbum>,
    val listArtists: (LibraryWindowRange) -> LibraryWindow<LibraryArtist>,
    val openAlbum: (LibraryAlbum) -> AlbumTrackList,
    val listAlbumTracks: (LibraryAlbum, LibraryWindowRange) -> LibraryWindow<LibraryTrack>,
    val openArtist: (LibraryArtist) -> ArtistTrackList = { artist ->
        ArtistTrackList(artist, LibraryWindow.empty())
    },
    val listArtistTracks: (LibraryArtist, LibraryWindowRange) -> LibraryWindow<LibraryTrack> =
        { _, _ -> LibraryWindow.empty() },
    val listFavourites: (LibraryWindowRange) -> LibraryWindow<LibraryTrack> =
        { LibraryWindow.empty() },
    val loadTrack: (Long, (LibraryTrack?) -> Unit) -> Unit,
    val playTracks: (PlaybackSelection, (String) -> Unit) -> Unit = { _, _ -> },
    val loadPlaybackSettings: () -> PlaybackSettingsUiState,
    val setEqualizerEnabled: (Boolean) -> PlaybackSettingsUiState,
    val replaceEqualizerCurve: (List<EqualizerCurvePoint>) -> PlaybackSettingsUiState,
    val setGaplessEnabled: (Boolean) -> PlaybackSettingsUiState,
    val selectTheme: (MobileThemeSelection, MobileTheme) -> MobileThemeSelection,
    val selectVisualizer: (MobileVisualizer) -> MobileVisualizer,
    val animationsEnabled: () -> Boolean,
    val observeAmbientScheduling: (Boolean) -> Unit,
)
