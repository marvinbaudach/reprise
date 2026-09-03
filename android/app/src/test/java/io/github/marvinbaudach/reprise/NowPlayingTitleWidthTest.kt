package io.github.marvinbaudach.reprise

import androidx.activity.ComponentActivity
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onRoot
import io.github.marvinbaudach.reprise.ui.theme.RepriseTheme
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import uniffi.reprise_android_ffi.AndroidColorScheme
import uniffi.reprise_android_ffi.AndroidPlaybackState

/**
 * The title block is laid out wider than the display on purpose: that surplus
 * is what lets the title travel faster than the cover while a swipe is in
 * flight. The text inside it must not inherit that width. Laid out against the
 * panel width a long title still "fits", so its ellipsis never fires and the
 * glyphs run off both edges of the screen instead.
 *
 * These tests measure the rendered title and artist rows against the display,
 * with a title long enough to have overflowed before the column was given the
 * screen's own width.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36], qualifiers = "w412dp-h916dp-port")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class NowPlayingTitleWidthTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    fun aLongTitleStaysInsideTheDisplay() {
        val surfaceState = MobileSurfaceViewModel()
        compose.setContent { TitleScene(LONG_TITLE, "Artist", surfaceState) }
        compose.waitForIdle()

        assertInsideTheDisplay("now-playing-title")
    }

    @Test
    fun aLongArtistNameStaysInsideTheDisplay() {
        val surfaceState = MobileSurfaceViewModel()
        compose.setContent { TitleScene("Song", LONG_ARTIST, surfaceState) }
        compose.waitForIdle()

        assertInsideTheDisplay("now-playing-artist")
    }

    private fun assertInsideTheDisplay(tag: String) {
        val screen = compose.onRoot().getUnclippedBoundsInRoot()
        val row = compose.onNodeWithTag(tag).getUnclippedBoundsInRoot()

        assertTrue(
            "$tag starts at ${row.left}, left of the display edge at ${screen.left}",
            row.left >= screen.left,
        )
        assertTrue(
            "$tag ends at ${row.right}, past the display edge at ${screen.right}",
            row.right <= screen.right,
        )

        // A row that overflows equally on both sides would pass the two checks
        // above only by accident of symmetry; one that is off-centre fails this
        // one even while it fits.
        val rowCentre = (row.left + row.right) / 2
        val screenCentre = (screen.left + screen.right) / 2
        assertTrue(
            "$tag is centred on $rowCentre, not on the display centre $screenCentre",
            kotlin.math.abs((rowCentre - screenCentre).value) < 1f,
        )
    }

    @Composable
    private fun TitleScene(
        title: String,
        artist: String,
        surfaceState: MobileSurfaceViewModel,
    ) {
        val theme = MobileThemeSelection(
            palette = MobileTheme.NOCTURNE,
            colorScheme = AndroidColorScheme.SYSTEM,
            dynamicAvailable = false,
        )
        RepriseTheme(theme, darkPalette = true) {
            CompositionLocalProvider(
                LocalAmbientMotionController provides AmbientMotionController(),
                LocalVisualSceneEngineFactory provides IdleSceneEngineFactory,
            ) {
                NowPlayingScene(
                    track = titleTrack(title, artist),
                    playback = PlaybackUiState(state = AndroidPlaybackState.PLAYING),
                    surfaceState = surfaceState,
                    visualizerOpacity = 0f,
                )
            }
        }
    }
}

/** The native visualiser has no library under Robolectric; the layout needs none. */
private object IdleSceneEngineFactory : VisualSceneEngineFactory {
    override fun create(): VisualSceneEngine = IdleSceneEngine
}

private object IdleSceneEngine : VisualSceneEngine {
    override fun setAccent(red: Float, green: Float, blue: Float) = Unit
    override fun setPlaying(playing: Boolean) = Unit
    override fun noteTrackChanged() = Unit
    override fun ingestBands(bands: FloatArray) = Unit
    override fun tick() = Unit
    override fun scene(width: Float, height: Float): List<Float> = emptyList()
    override fun close() = Unit
}

private fun titleTrack(title: String, artist: String) = LibraryTrack(
    id = 31,
    uri = "content://provider/song.flac",
    title = title,
    artist = artist,
    album = "Album",
    durationMs = 180_000,
    playCount = 0,
    rating = 0,
)

private const val LONG_TITLE =
    "Everything That Happens Will Happen Today (Extended Remaster Version)"

private const val LONG_ARTIST =
    "The Ridiculously Long Collective Name Orchestra And Their Friends"
