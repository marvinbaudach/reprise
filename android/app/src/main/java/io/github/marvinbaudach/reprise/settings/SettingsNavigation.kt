package io.github.marvinbaudach.reprise.settings

import androidx.activity.compose.BackHandler
import androidx.compose.animation.core.tween
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.navigation.NavGraphBuilder
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.currentBackStackEntryAsState
import androidx.navigation.compose.rememberNavController
import io.github.marvinbaudach.reprise.BuildConfig
import io.github.marvinbaudach.reprise.EqualizerCurvePoint
import io.github.marvinbaudach.reprise.MobileTheme
import io.github.marvinbaudach.reprise.MobileThemeSelection
import io.github.marvinbaudach.reprise.PlaybackSettingsScreen
import io.github.marvinbaudach.reprise.PlaybackSettingsUiState

/** How long a settings page takes to slide in or out, in milliseconds. */
internal const val SETTINGS_PAGE_SLIDE_MS = 300

/**
 * The page behind moves a quarter as far as the page in front, so the two
 * read as one stack rather than two slides passing each other.
 */
private const val PAGE_BEHIND_PARALLAX = 4

/** A settings-only graph whose lifetime is exactly the overlay's lifetime. */
@Composable
internal fun SettingsNavigation(
    state: PlaybackSettingsUiState,
    titleCount: Long,
    albumCount: Long,
    artistCount: Long,
    folderName: String?,
    themeSelection: MobileThemeSelection,
    active: Boolean = true,
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

    BackHandler(enabled = active && (route == null || route == SettingsRoute.OVERVIEW.route)) {
        close()
    }

    // Pages slide, they never fade. The graph's default is a crossfade, and
    // no page here paints a background of its own, so for most of a second
    // both pages showed through each other. Sliding keeps the front page
    // whole, and the opaque surface around each page keeps what is behind it
    // hidden until it has left. The graph itself draws the front page on top,
    // arriving and leaving alike, so the parallax needs no z-order of its own.
    NavHost(
        navController = navController,
        startDestination = SettingsRoute.OVERVIEW.route,
        enterTransition = {
            slideInHorizontally(tween(SETTINGS_PAGE_SLIDE_MS)) { width -> width }
        },
        exitTransition = {
            slideOutHorizontally(tween(SETTINGS_PAGE_SLIDE_MS)) { width ->
                -width / PAGE_BEHIND_PARALLAX
            }
        },
        popEnterTransition = {
            slideInHorizontally(tween(SETTINGS_PAGE_SLIDE_MS)) { width ->
                -width / PAGE_BEHIND_PARALLAX
            }
        },
        popExitTransition = {
            slideOutHorizontally(tween(SETTINGS_PAGE_SLIDE_MS)) { width -> width }
        },
    ) {
        page(SettingsRoute.OVERVIEW) {
            SettingsOverview(
                titleCount = titleCount,
                themeSelection = themeSelection,
                versionName = BuildConfig.VERSION_NAME,
                error = state.error,
                close = close,
                open = { destination -> navController.navigate(destination.route) },
            )
        }
        page(SettingsRoute.LIBRARY) {
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
        page(SettingsRoute.AUDIO) {
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
        page(SettingsRoute.APPEARANCE) {
            AppearanceSettingsPage(
                themeSelection = themeSelection,
                selectTheme = selectTheme,
                back = { navController.navigateUp() },
            )
        }
        page(SettingsRoute.ABOUT) {
            AboutSettingsPage(back = { navController.navigateUp() })
        }
    }
}

/** A destination that covers whatever the graph draws it over. */
private fun NavGraphBuilder.page(route: SettingsRoute, content: @Composable () -> Unit) {
    composable(route.route) {
        Surface(
            modifier = Modifier.fillMaxSize(),
            color = MaterialTheme.colorScheme.background,
        ) {
            content()
        }
    }
}
