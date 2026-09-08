package io.github.marvinbaudach.reprise

import android.net.Uri
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.reprise_android_ffi.AndroidStoredLibraryDestination

/** A JVM-replaceable library edge; activity, service, ViewModel and UI stay real. */
internal interface MainActivitySurfaceProvider {
    fun mainActivitySurface(): MainActivitySurfaceDependencies
}

internal data class MainActivitySurfaceDependencies(
    val initialTheme: MobileThemeSelection,
    val initialState: LibraryScreenState,
    val initialStoredDestination: AndroidStoredLibraryDestination =
        AndroidStoredLibraryDestination.Titles,
    val rememberBrowseTab: (BrowseTab) -> Unit = {},
    val artwork: () -> TrackArtwork?,
    val playbackControls: PlaybackControls,
    val trackAnalysis: TrackAnalysisPort,
    val chooseFolder: (Uri, (LibraryScreenState) -> Unit) -> Unit,
    val rescan: ((LibraryScreenState) -> Unit) -> Unit,
    val searchTitles: suspend (String, LibraryWindowRange) -> LibraryWindow<LibraryTrack>,
    val listArtists: suspend (LibraryWindowRange) -> LibraryWindow<LibraryArtist>,
    val searchArtists: suspend (String, LibraryWindowRange) -> LibraryWindow<LibraryArtist>,
    val openAlbum: suspend (LibraryAlbum) -> AlbumTrackList,
    val listAlbumTracks:
        suspend (LibraryAlbum, LibraryWindowRange) -> LibraryWindow<LibraryTrack>,
    val openArtist: suspend (LibraryArtist) -> ArtistTrackList = { artist ->
        ArtistTrackList(artist = artist)
    },
    val listArtistTracks:
        suspend (LibraryArtist, LibraryWindowRange) -> LibraryWindow<LibraryTrack> =
        { _, _ -> LibraryWindow.empty() },
    val listArtistAlbums:
        suspend (LibraryArtist, LibraryWindowRange) -> LibraryWindow<LibraryAlbum> =
        { _, _ -> LibraryWindow.empty() },
    val listArtistUntaggedTracks:
        suspend (LibraryArtist, LibraryWindowRange) -> LibraryWindow<LibraryTrack> =
        { _, _ -> LibraryWindow.empty() },
    val loadTrack: (Long, (LibraryTrack?) -> Unit) -> Unit,
    val playTracks: (PlaybackSelection, (String) -> Unit) -> Unit = { _, _ -> },
    val loadPlaybackSettings: () -> PlaybackSettingsUiState,
    val setEqualizerEnabled: (Boolean) -> PlaybackSettingsUiState,
    val replaceEqualizerCurve: (List<EqualizerCurvePoint>) -> PlaybackSettingsUiState,
    val setGaplessEnabled: (Boolean) -> PlaybackSettingsUiState,
    val selectTheme: (MobileThemeSelection, MobileTheme) -> MobileThemeSelection,
    val onlineSourcesEnabled: () -> Boolean = { false },
    val setOnlineSourcesEnabled: (Boolean) -> Result<Unit> = { Result.success(Unit) },
    val animationsEnabled: () -> Boolean,
    val observeAmbientScheduling: (Boolean) -> Unit,
    val libraryPerformanceObserver: LibraryPerformanceObserver = NoOpLibraryPerformanceObserver,
)

internal fun <A, R> offMainLibraryRead(
    query: suspend (A) -> R,
): suspend (A) -> R = { argument ->
    withContext(Dispatchers.IO) {
        requireOffMainThread("Library read")
        query(argument)
    }
}

internal fun <A, B, R> offMainLibraryRead(
    query: suspend (A, B) -> R,
): suspend (A, B) -> R = { first, second ->
    withContext(Dispatchers.IO) {
        requireOffMainThread("Library read")
        query(first, second)
    }
}
