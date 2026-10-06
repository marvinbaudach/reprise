package io.github.marvinbaudach.reprise

import android.os.Looper
import androidx.activity.ComponentActivity
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.test.click
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performTouchInput
import io.github.marvinbaudach.reprise.ui.theme.RepriseTheme
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import uniffi.reprise_android_ffi.AndroidColorScheme
import uniffi.reprise_android_ffi.AndroidPlaybackState
import uniffi.reprise_android_ffi.AndroidStoredVisualizer
import uniffi.reprise_android_ffi.AndroidVisualizerChoice

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36], qualifiers = "w500dp-h1000dp")
class NowPlayingSheetLightTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    fun theFilmTakesTheSlowClockWhileTheCoverAndBarsTakeTheFastOne() {
        val preference = LightTestVisualizerPreference()
        val controller = AmbientMotionController()
        var lastOpacity = 0f
        var lastLight = 0f
        compose.mainClock.autoAdvance = false
        compose.setContent {
            val theme = MobileThemeSelection(
                palette = MobileTheme.NOCTURNE,
                colorScheme = AndroidColorScheme.SYSTEM,
                dynamicAvailable = false,
            )
            RepriseTheme(theme, darkPalette = true) {
                CompositionLocalProvider(
                    LocalPlaybackControls provides DisconnectedPlaybackControls,
                    LocalVisualizerPreference provides preference,
                    LocalAmbientMotionController provides controller,
                ) {
                    NowPlayingSheet(
                        track = lightTestTrack(),
                        playback = lightTestPlayback(),
                        close = {},
                        onSceneLightObserved = { opacity, light ->
                            lastOpacity = opacity
                            lastLight = light
                        },
                    )
                }
            }
        }
        compose.runOnIdle {
            controller.runtimeChanged(resumed = true, screenInteractive = true, animationsEnabled = true)
        }
        compose.mainClock.advanceTimeBy(DISPLAY_FRAME_MS * 4)

        compose.onNodeWithTag("now-playing-gestures").performTouchInput {
            click(Offset(width * 0.5f, height * 0.34f))
        }
        // The cover tap first waits out its 300 ms double-tap window. Only then
        // does the visibility change start either animation clock.
        advanceComposeAndMainLooper(TAP_RESOLUTION_MS)
        advanceComposeAndMainLooper(VISUALIZER_CROSSFADE_MS.toLong())

        assertEquals(
            "the bars finish their own crossfade at VISUALIZER_CROSSFADE_MS",
            1f,
            lastOpacity,
            0.01f,
        )
        assertTrue(
            "the film must still be on its way at the bars' faster finish: $lastLight",
            lastLight < 1f,
        )

        advanceComposeAndMainLooper((FOG_CROSSFADE_MS - VISUALIZER_CROSSFADE_MS).toLong())

        assertEquals(
            "the film finishes at FOG_CROSSFADE_MS from the shared start",
            1f,
            lastLight,
            0.01f,
        )
    }

    private fun advanceComposeAndMainLooper(milliseconds: Long) {
        compose.mainClock.advanceTimeBy(milliseconds)
        shadowOf(Looper.getMainLooper()).idle()
        compose.waitForIdle()
    }
}

private class LightTestVisualizerPreference : VisualizerPreference {
    override fun visualizerSetting(): AndroidStoredVisualizer = AndroidStoredVisualizer.Cover

    override fun setVisualizer(
        choice: AndroidVisualizerChoice,
        report: (Result<Unit>) -> Unit,
    ) = report(Result.success(Unit))
}

private fun lightTestPlayback() = PlaybackUiState(
    ready = true,
    state = AndroidPlaybackState.PAUSED,
    currentIndex = 0,
    currentTrackId = 912,
    currentTrackUri = "content://provider/document/912.flac",
    positionMs = 20_000,
    durationMs = 100_000,
)

private fun lightTestTrack() = LibraryTrack(
    id = 912,
    uri = "content://provider/document/912.flac",
    title = "Light clock",
    artist = "Artist",
    album = "Album",
    durationMs = 100_000,
    playCount = 0,
    rating = 0,
)

/** Past the 300 ms double-tap window, with headroom for a few frames. */
private const val TAP_RESOLUTION_MS = 350L
private const val DISPLAY_FRAME_MS = 16L
