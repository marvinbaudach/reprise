package io.github.marvinbaudach.reprise.widget

import android.content.Intent
import androidx.compose.ui.unit.DpSize
import androidx.test.core.app.ApplicationProvider
import androidx.glance.appwidget.testing.unit.assertHasRunCallbackClickAction
import androidx.glance.appwidget.testing.unit.assertHasStartActivityClickAction
import androidx.glance.appwidget.testing.unit.runGlanceAppWidgetUnitTest
import androidx.glance.testing.unit.hasContentDescription
import androidx.glance.testing.unit.hasText
import io.github.marvinbaudach.reprise.MainActivity
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

private val PLAYING = WidgetNowPlaying(4, "Nightcall", "Kavinsky", isPlaying = true, artworkPath = null)

/** What the widget draws at each size, and which tap does what. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class RepriseWidgetContentTest {
    private fun openApp() = Intent(ApplicationProvider.getApplicationContext(), MainActivity::class.java)

    @Test
    fun theWideWidgetShowsTitleAndArtist() = runGlanceAppWidgetUnitTest {
        setContext(ApplicationProvider.getApplicationContext())
        setAppWidgetSize(WIDE_SIZE)
        provideComposable { RepriseWidgetContent(PLAYING, covers = WidgetCovers.None) }

        onNode(hasText("Nightcall")).assertExists()
        onNode(hasText("Kavinsky")).assertExists()
    }

    @Test
    fun os_10_the_wide_widgets_buttons_send_media_commands_to_the_service() = runGlanceAppWidgetUnitTest {
        setContext(ApplicationProvider.getApplicationContext())
        setAppWidgetSize(WIDE_SIZE)
        provideComposable { RepriseWidgetContent(PLAYING, covers = WidgetCovers.None) }

        onNode(hasContentDescription("Previous track"))
            .assertHasRunCallbackClickAction<PreviousAction>()
        onNode(hasContentDescription("Pause"))
            .assertHasRunCallbackClickAction<TogglePlayAction>()
        onNode(hasContentDescription("Next track"))
            .assertHasRunCallbackClickAction<NextAction>()
    }

    @Test
    fun aPausedWidgetOffersPlay() = runGlanceAppWidgetUnitTest {
        setContext(ApplicationProvider.getApplicationContext())
        setAppWidgetSize(WIDE_SIZE)
        provideComposable { RepriseWidgetContent(PLAYING.copy(isPlaying = false), covers = WidgetCovers.None) }

        onNode(hasContentDescription("Play"))
            .assertHasRunCallbackClickAction<TogglePlayAction>()
    }

    @Test
    fun whenThereIsNoQueueToResumeThePlayButtonOpensTheAppInstead() = runGlanceAppWidgetUnitTest {
        setContext(ApplicationProvider.getApplicationContext())
        setAppWidgetSize(WIDE_SIZE)
        val ended = PLAYING.copy(isPlaying = false, canResume = false)
        provideComposable { RepriseWidgetContent(ended, covers = WidgetCovers.None) }

        onNode(hasContentDescription("Play")).assertHasStartActivityClickAction(openApp())
        onNode(hasContentDescription("Next track")).assertHasStartActivityClickAction(openApp())
        onNode(hasContentDescription("Previous track")).assertHasStartActivityClickAction(openApp())
    }

    @Test
    fun theSquareWidgetsPlayButtonOpensTheAppWhenThereIsNothingToResume() = runGlanceAppWidgetUnitTest {
        setContext(ApplicationProvider.getApplicationContext())
        setAppWidgetSize(SQUARE_SIZE)
        val ended = PLAYING.copy(isPlaying = false, canResume = false)
        provideComposable { RepriseWidgetContent(ended, covers = WidgetCovers.None) }

        onNode(hasContentDescription("Play")).assertHasStartActivityClickAction(openApp())
    }

    @Test
    fun tappingTheCoverOpensTheAppInsteadOfSendingACommand() = runGlanceAppWidgetUnitTest {
        setContext(ApplicationProvider.getApplicationContext())
        setAppWidgetSize(WIDE_SIZE)
        provideComposable { RepriseWidgetContent(PLAYING, covers = WidgetCovers.None) }

        onNode(hasContentDescription("Open Reprise"))
            .assertHasStartActivityClickAction(openApp())
    }

    @Test
    fun os_10_the_square_widget_is_a_cover_with_one_play_pause_button() = runGlanceAppWidgetUnitTest {
        setContext(ApplicationProvider.getApplicationContext())
        setAppWidgetSize(SQUARE_SIZE)
        provideComposable { RepriseWidgetContent(PLAYING, covers = WidgetCovers.None) }

        onNode(hasContentDescription("Open Reprise"))
            .assertHasStartActivityClickAction(openApp())
        onNode(hasContentDescription("Pause"))
            .assertHasRunCallbackClickAction<TogglePlayAction>()
        onNode(hasText("Nightcall")).assertDoesNotExist()
        onNode(hasContentDescription("Next track")).assertDoesNotExist()
        onNode(hasContentDescription("Previous track")).assertDoesNotExist()
    }

    @Test
    fun os_10_before_anything_was_played_the_widget_shows_the_app_name_and_opens_the_app() = runGlanceAppWidgetUnitTest {
        setContext(ApplicationProvider.getApplicationContext())
        setAppWidgetSize(WIDE_SIZE)
        provideComposable { RepriseWidgetContent(WidgetNowPlaying.Empty, covers = WidgetCovers.None) }

        onNode(hasText("Reprise")).assertExists()
        onNode(hasContentDescription("Next track")).assertDoesNotExist()
        onNode(hasContentDescription("Play")).assertDoesNotExist()
        onNode(hasContentDescription("Open Reprise")).assertHasStartActivityClickAction(openApp())
    }

    @Test
    fun aTrackWithoutATitleStillGetsAWidgetText() = runGlanceAppWidgetUnitTest {
        setContext(ApplicationProvider.getApplicationContext())
        setAppWidgetSize(WIDE_SIZE)
        provideComposable { RepriseWidgetContent(PLAYING.copy(title = "", artist = ""), covers = WidgetCovers.None) }

        onNode(hasText("Unknown title")).assertExists()
    }

    @Test
    fun theLayoutFollowsTheSizeTheLauncherGives() {
        assertTrue(isWide(WIDE_SIZE))
        assertFalse(isWide(SQUARE_SIZE))
        assertFalse(isWide(DpSize(WIDE_SIZE.width, SQUARE_SIZE.height)))
        assertEquals(R_PLAY, playIcon(PLAYING.copy(isPlaying = false)))
    }
}

private val R_PLAY = io.github.marvinbaudach.reprise.R.drawable.ic_widget_play
