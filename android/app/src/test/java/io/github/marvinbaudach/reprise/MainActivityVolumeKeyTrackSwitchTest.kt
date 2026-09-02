package io.github.marvinbaudach.reprise

import android.os.Looper
import android.os.SystemClock
import android.view.KeyCharacterMap
import android.view.KeyEvent
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(
    sdk = [36],
    qualifiers = "w412dp-h916dp-port",
    application = ConfigurationTestApplication::class,
)
class MainActivityVolumeKeyTrackSwitchTest {
    @get:Rule
    val compose = createAndroidComposeRule<MainActivity>()

    private val application: ConfigurationTestApplication
        get() = RuntimeEnvironment.getApplication() as ConfigurationTestApplication

    @After
    fun releaseTheService() {
        application.releaseService()
    }

    @Test
    fun holdingVolumeKeysWhilePlayingUsesTheInjectedTransportControls() {
        publishPlayingTrack()

        dispatchLongPress(KeyEvent.KEYCODE_VOLUME_UP)
        dispatchLongPress(KeyEvent.KEYCODE_VOLUME_DOWN)

        assertEquals(listOf("next", "previous"), application.controls.transportCommands)
    }

    @Test
    fun shortVolumePressWhilePlayingDoesNotSendATransportCommand() {
        publishPlayingTrack()

        dispatchShortPress(KeyEvent.KEYCODE_VOLUME_UP)

        assertEquals(emptyList<String>(), application.controls.transportCommands)
    }

    @Test
    fun holdingVolumeKeysWhileStoppedDoesNotSendATransportCommand() {
        dispatchLongPress(KeyEvent.KEYCODE_VOLUME_UP)
        dispatchLongPress(KeyEvent.KEYCODE_VOLUME_DOWN)

        assertEquals(emptyList<String>(), application.controls.transportCommands)
    }

    @Test
    fun repeatDownWhilePlayingIsConsumed() {
        publishPlayingTrack()
        val downTime = SystemClock.uptimeMillis()
        compose.activity.dispatchKeyEvent(
            keyEvent(downTime, downTime, KeyEvent.ACTION_DOWN, KeyEvent.KEYCODE_VOLUME_UP),
        )

        val repeatConsumed = compose.activity.dispatchKeyEvent(
            keyEvent(
                downTime = downTime,
                eventTime = downTime + 1,
                action = KeyEvent.ACTION_DOWN,
                keyCode = KeyEvent.KEYCODE_VOLUME_UP,
                repeatCount = 1,
            ),
        )

        assertTrue(repeatConsumed)
    }

    @Test
    fun canceledUpAfterLongPressWhilePlayingIsConsumed() {
        publishPlayingTrack()

        val upConsumed = dispatchLongPress(KeyEvent.KEYCODE_VOLUME_UP)

        assertTrue(upConsumed)
    }

    private fun publishPlayingTrack() {
        application.service.publish(m9bSnapshot(1))
        shadowOf(Looper.getMainLooper()).idle()
        compose.waitForIdle()
    }

    private fun dispatchShortPress(keyCode: Int) {
        val downTime = SystemClock.uptimeMillis()
        compose.activity.dispatchKeyEvent(keyEvent(downTime, downTime, KeyEvent.ACTION_DOWN, keyCode))
        compose.activity.dispatchKeyEvent(keyEvent(downTime, downTime + 1, KeyEvent.ACTION_UP, keyCode))
    }

    private fun dispatchLongPress(keyCode: Int): Boolean {
        val downTime = SystemClock.uptimeMillis()
        compose.activity.dispatchKeyEvent(keyEvent(downTime, downTime, KeyEvent.ACTION_DOWN, keyCode))
        compose.activity.dispatchKeyEvent(
            keyEvent(
                downTime = downTime,
                eventTime = downTime + 1,
                action = KeyEvent.ACTION_DOWN,
                keyCode = keyCode,
                repeatCount = 1,
                flags = KeyEvent.FLAG_LONG_PRESS,
            ),
        )
        val upConsumed = compose.activity.dispatchKeyEvent(
            keyEvent(downTime, downTime + 2, KeyEvent.ACTION_UP, keyCode),
        )
        return upConsumed
    }

    private fun keyEvent(
        downTime: Long,
        eventTime: Long,
        action: Int,
        keyCode: Int,
        repeatCount: Int = 0,
        flags: Int = 0,
    ) = KeyEvent(
        downTime,
        eventTime,
        action,
        keyCode,
        repeatCount,
        0,
        KeyCharacterMap.VIRTUAL_KEYBOARD,
        0,
        flags,
    )
}
