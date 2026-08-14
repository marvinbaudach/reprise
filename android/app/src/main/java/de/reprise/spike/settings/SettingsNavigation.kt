package de.reprise.spike.settings

import androidx.activity.compose.BackHandler
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.currentBackStackEntryAsState
import androidx.navigation.compose.rememberNavController
import de.reprise.spike.BuildConfig
import de.reprise.spike.EqualizerCurvePoint
import de.reprise.spike.MobileTheme
import de.reprise.spike.MobileThemeSelection
import de.reprise.spike.PlaybackSettingsScreen
import de.reprise.spike.PlaybackSettingsUiState

/** A settings-only graph whose lifetime is exactly the overlay's lifetime. */
@Composable
internal fun SettingsNavigation(
    state: PlaybackSettingsUiState,
    titleCount: Long,
    albumCount: Long,
    artistCount: Long,
    folderName: String?,
    themeSelection: MobileThemeSelection,
    onlineSourcesEnabled: Boolean = false,
    setOnlineSourcesEnabled: (Boolean) -> Unit = {},
    close: () -> Unit,
    chooseFolder: () -> Unit,
    rescan: () -> Unit,
    setEqualizerEnabled: (Boolean) -> Unit,
    replaceEqualizerCurve: (List<EqualizerCurvePoint>) -> Unit,
    setGaplessEnabled: (Boolean) -> Unit,
    selectTheme: (MobileTheme) -> Unit,
) {
    val navController = rememberNavController()
    val entry by navController.currentBackStackEntryAsState()
    val route = entry?.destination?.route

    BackHandler(enabled = route == null || route == SettingsRoute.OVERVIEW.route) {
        close()
    }

    NavHost(
        navController = navController,
        startDestination = SettingsRoute.OVERVIEW.route,
    ) {
        composable(SettingsRoute.OVERVIEW.route) {
            SettingsOverview(
                titleCount = titleCount,
                themeSelection = themeSelection,
                onlineSourcesEnabled = onlineSourcesEnabled,
                versionName = BuildConfig.VERSION_NAME,
                error = state.error,
                close = close,
                open = { destination -> navController.navigate(destination.route) },
            )
        }
        composable(SettingsRoute.LIBRARY.route) {
            LibrarySettingsPage(
                titleCount = titleCount,
                albumCount = albumCount,
                artistCount = artistCount,
                folderName = folderName,
                back = { navController.navigateUp() },
                // Both of these hand the library a new catalogue, and the
                // screen that reports on it replaces the one this overlay is
                // drawn inside — so the overlay would come down anyway, in the
                // middle of a scan, without anyone having decided it. Leaving
                // deliberately is the same movement with an author: the scan is
                // watched where scans are watched, and the count that raised
                // the suspicion is the first thing standing there afterwards.
                chooseFolder = {
                    close()
                    chooseFolder()
                },
                rescan = {
                    close()
                    rescan()
                },
            )
        }
        composable(SettingsRoute.AUDIO.route) {
            PlaybackSettingsScreen(
                state = state,
                themeSelection = themeSelection,
                close = { navController.navigateUp() },
                setEqualizerEnabled = setEqualizerEnabled,
                replaceEqualizerCurve = replaceEqualizerCurve,
                setGaplessEnabled = setGaplessEnabled,
                selectTheme = selectTheme,
                pageTitle = "Audio",
                backContentDescription = "Back to Settings",
            )
        }
        composable(SettingsRoute.APPEARANCE.route) {
            AppearanceSettingsPage(
                themeSelection = themeSelection,
                selectTheme = selectTheme,
                back = { navController.navigateUp() },
            )
        }
        composable(SettingsRoute.ONLINE_SOURCES.route) {
            OnlineSourcesSettingsPage(
                enabled = onlineSourcesEnabled,
                setEnabled = setOnlineSourcesEnabled,
                back = { navController.navigateUp() },
            )
        }
        composable(SettingsRoute.ABOUT.route) {
            AboutSettingsPage(back = { navController.navigateUp() })
        }
    }
}
