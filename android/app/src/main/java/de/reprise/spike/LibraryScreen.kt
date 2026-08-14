package de.reprise.spike

import android.net.Uri
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue

@Composable
internal fun LibraryScreen(
    initialState: LibraryScreenState,
    playback: PlaybackUiState,
    playbackSettingsRevision: Long,
    surfaceLayout: SurfaceLayout,
    surfaceState: MobileSurfaceViewModel,
    chooseFolder: (Uri, (LibraryScreenState) -> Unit) -> Unit,
    rescan: ((LibraryScreenState) -> Unit) -> Unit,
    searchTitles: (String, LibraryWindowRange) -> LibraryWindow<LibraryTrack>,
    searchAlbums: (String, LibraryWindowRange) -> LibraryWindow<LibraryAlbum>,
    listArtists: (LibraryWindowRange) -> LibraryWindow<LibraryArtist>,
    searchArtists: (String, LibraryWindowRange) -> LibraryWindow<LibraryArtist>,
    openAlbum: (LibraryAlbum) -> AlbumTrackList,
    listAlbumTracks: (LibraryAlbum, LibraryWindowRange) -> LibraryWindow<LibraryTrack>,
    openArtist: (LibraryArtist) -> ArtistTrackList,
    listArtistTracks: (LibraryArtist, LibraryWindowRange) -> LibraryWindow<LibraryTrack>,
    listArtistAlbums: (LibraryArtist, LibraryWindowRange) -> LibraryWindow<LibraryAlbum>,
    listArtistUntaggedTracks:
        (LibraryArtist, LibraryWindowRange) -> LibraryWindow<LibraryTrack>,
    loadTrack: (Long, (LibraryTrack?) -> Unit) -> Unit,
    playTracks: (PlaybackSelection, (String) -> Unit) -> Unit,
    loadPlaybackSettings: () -> PlaybackSettingsUiState,
    setEqualizerEnabled: (Boolean) -> PlaybackSettingsUiState,
    replaceEqualizerCurve: (List<EqualizerCurvePoint>) -> PlaybackSettingsUiState,
    setGaplessEnabled: (Boolean) -> PlaybackSettingsUiState,
    onlineSourcesEnabled: Boolean,
    setOnlineSourcesEnabled: (Boolean) -> Unit,
    themeSelection: MobileThemeSelection,
    selectTheme: (MobileTheme) -> Unit,
) {
    var state by remember { mutableStateOf(initialState) }
    val folderPicker = rememberLauncherForActivityResult(
        contract = ActivityResultContracts.OpenDocumentTree(),
    ) { treeUri ->
        if (treeUri != null) {
            chooseFolder(treeUri) { state = it }
        }
    }
    val launchFolderPicker = { folderPicker.launch(folderPickerInitialUri()) }

    when (val current = state) {
        is LibraryScreenState.NoFolder -> NoFolderScreen(
            message = current.message,
            chooseFolder = launchFolderPicker,
        )
        LibraryScreenState.TreeUnreadable -> TreeUnreadableScreen(
            chooseFolder = launchFolderPicker,
        )
        is LibraryScreenState.Scanning -> ScanningScreen(current)
        is LibraryScreenState.Browse -> BrowseScreen(
            state = current,
            playback = playback,
            playbackSettingsRevision = playbackSettingsRevision,
            surfaceLayout = surfaceLayout,
            surfaceState = surfaceState,
            chooseFolder = launchFolderPicker,
            rescan = { rescan { state = it } },
            searchTitles = searchTitles,
            searchAlbums = searchAlbums,
            listArtists = listArtists,
            searchArtists = searchArtists,
            openAlbum = openAlbum,
            listAlbumTracks = listAlbumTracks,
            openArtist = openArtist,
            listArtistTracks = listArtistTracks,
            listArtistAlbums = listArtistAlbums,
            listArtistUntaggedTracks = listArtistUntaggedTracks,
            loadTrack = loadTrack,
            playTracks = playTracks,
            loadPlaybackSettings = loadPlaybackSettings,
            setEqualizerEnabled = setEqualizerEnabled,
            replaceEqualizerCurve = replaceEqualizerCurve,
            setGaplessEnabled = setGaplessEnabled,
            onlineSourcesEnabled = onlineSourcesEnabled,
            setOnlineSourcesEnabled = setOnlineSourcesEnabled,
            themeSelection = themeSelection,
            selectTheme = selectTheme,
        )
    }
}
