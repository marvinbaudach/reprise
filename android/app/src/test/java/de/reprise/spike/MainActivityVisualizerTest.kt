package de.reprise.spike

import android.os.Looper
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTouchInput
import org.junit.After
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import uniffi.reprise_android_ffi.AndroidPlaybackSnapshot
import uniffi.reprise_android_ffi.AndroidPlaybackState
import uniffi.reprise_android_ffi.AndroidRepeatMode

@RunWith(RobolectricTestRunner::class)
@Config(
    sdk = [36],
    qualifiers = "w412dp-h916dp-port",
    application = ConfigurationTestApplication::class,
)
class MainActivityVisualizerTest {
    @get:Rule
    val compose = createAndroidComposeRule<MainActivity>()

    private val application: ConfigurationTestApplication
        get() = RuntimeEnvironment.getApplication() as ConfigurationTestApplication

    @After
    fun releaseTheService() {
        application.releaseService()
    }

    @Test
    fun retired_visualizer_modes_are_absent_from_now_playing_and_appearance_settings() {
        application.service.publish(m9bSnapshot(trackId = 1))
        shadowOf(Looper.getMainLooper()).idle()
        compose.waitForIdle()
        compose.onNodeWithTag("library-mini-player").performClick()

        compose.onNodeWithTag("now-playing-cover").assertExists()
        compose.onNodeWithTag("visualizer-bar-COVER").assertDoesNotExist()
        compose.onNodeWithTag("visualizer-bar-AMBIENT").assertDoesNotExist()
        compose.onNodeWithTag("visualizer-surface").assertDoesNotExist()
        compose.onNodeWithTag("now-playing-cover").performTouchInput {
            down(center)
            advanceEventTime(600)
            up()
        }
        compose.onNodeWithTag("visualizer-menu-COVER").assertDoesNotExist()
        compose.onNodeWithTag("visualizer-menu-AMBIENT").assertDoesNotExist()

        compose.onNodeWithContentDescription("Collapse Now Playing").performClick()
        compose.onNodeWithContentDescription("Library actions").performClick()
        compose.onNodeWithText("Settings").performClick()
        compose.onNodeWithContentDescription("Open Appearance").performClick()
        compose.onNodeWithText("Visualizer").assertDoesNotExist()
        compose.onNodeWithTag("settings-visualizer-COVER").assertDoesNotExist()
        compose.onNodeWithTag("settings-visualizer-AMBIENT").assertDoesNotExist()
    }
}

internal fun m9bSnapshot(trackId: Long) = AndroidPlaybackSnapshot(
    state = AndroidPlaybackState.PLAYING,
    currentIndex = 0u,
    currentTrackId = trackId,
    currentTrackUri = "content://provider/document/$trackId.flac",
    positionMs = 12_000,
    durationMs = 120_000,
    automaticAdvanceCount = 0u,
    shuffled = false,
    repeat = AndroidRepeatMode.OFF,
    error = null,
)
