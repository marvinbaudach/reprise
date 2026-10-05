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
        provideComposable { RepriseWidgetContent(PLAYING, cover = null) }

        onNode(hasText("Nightcall")).assertExists()
        onNode(hasText("Kavinsky")).assertExists()
    }

    @Test
    fun theWideWidgetsButtonsSendMediaCommandsToTheService() = runGlanceAppWidgetUnitTest {
        setContext(ApplicationProvider.getApplicationContext())
        setAppWidgetSize(WIDE_SIZE)
        provideComposable { RepriseWidgetContent(PLAYING, cover = null) }

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
        provideComposable { RepriseWidgetContent(PLAYING.copy(isPlaying = false), cover = null) }

        onNode(hasContentDescription("Play"))
            .assertHasRunCallbackClickAction<TogglePlayAction>()
    }

    @Test
    fun tappingTheCoverOpensTheAppInsteadOfSendingACommand() = runGlanceAppWidgetUnitTest {
        setContext(ApplicationProvider.getApplicationContext())
        setAppWidgetSize(WIDE_SIZE)
        provideComposable { RepriseWidgetContent(PLAYING, cover = null) }

        onNode(hasContentDescription("Open Reprise"))
            .assertHasStartActivityClickAction(openApp())
    }

    @Test
    fun theSquareWidgetIsACoverWithOnePlayPauseButton() = runGlanceAppWidgetUnitTest {
        setContext(ApplicationProvider.getApplicationContext())
        setAppWidgetSize(SQUARE_SIZE)
        provideComposable { RepriseWidgetContent(PLAYING, cover = null) }

        onNode(hasContentDescription("Open Reprise"))
            .assertHasStartActivityClickAction(openApp())
        onNode(hasContentDescription("Pause"))
            .assertHasRunCallbackClickAction<TogglePlayAction>()
        onNode(hasText("Nightcall")).assertDoesNotExist()
        onNode(hasContentDescription("Next track")).assertDoesNotExist()
        onNode(hasContentDescription("Previous track")).assertDoesNotExist()
    }

    @Test
    fun beforeAnythingWasPlayedTheWidgetShowsTheAppNameAndOpensTheApp() = runGlanceAppWidgetUnitTest {
        setContext(ApplicationProvider.getApplicationContext())
        setAppWidgetSize(WIDE_SIZE)
        provideComposable { RepriseWidgetContent(WidgetNowPlaying.Empty, cover = null) }

        onNode(hasText("Reprise")).assertExists()
        onNode(hasContentDescription("Next track")).assertDoesNotExist()
        onNode(hasContentDescription("Play")).assertDoesNotExist()
        onNode(hasContentDescription("Open Reprise")).assertHasStartActivityClickAction(openApp())
    }

    @Test
    fun aTrackWithoutATitleStillGetsAWidgetText() = runGlanceAppWidgetUnitTest {
        setContext(ApplicationProvider.getApplicationContext())
        setAppWidgetSize(WIDE_SIZE)
        provideComposable { RepriseWidgetContent(PLAYING.copy(title = "", artist = ""), cover = null) }

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
