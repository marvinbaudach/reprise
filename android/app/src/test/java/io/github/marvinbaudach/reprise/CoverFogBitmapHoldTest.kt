package io.github.marvinbaudach.reprise

import android.graphics.Bitmap
import android.graphics.Color
import androidx.activity.ComponentActivity
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import kotlinx.coroutines.CoroutineDispatcher
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import kotlin.coroutines.CoroutineContext

/**
 * [rememberCoverFogBitmap] used to reset to `null` (or a cache hit) on every
 * artwork change, so a track without a cache hit read as a hard cut to black
 * for however long the blur took. It now keeps the last finished fog instead.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class CoverFogBitmapHoldTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    fun the_first_fog_arrives_from_the_blur_when_nothing_is_cached() {
        val cache = ArtworkCache()
        val artwork = solidBitmap(Color.WHITE).asImageBitmap()
        val observations = mutableListOf<CoverFogBitmap?>()

        compose.setContent {
            val fog = rememberCoverFogBitmap(
                artwork = artwork,
                fallback = androidx.compose.ui.graphics.Color.Magenta,
                cache = cache,
            )
            SideEffect { observations += fog }
        }

        compose.waitUntil(timeoutMillis = 5_000) { observations.last() != null }
    }

    @Test
    fun the_previous_fog_is_held_while_the_next_one_is_blurred() {
        val cache = ArtworkCache()
        val dispatcher = QueuedFogDispatcher()
        val cachedArtwork = solidBitmap(Color.WHITE).asImageBitmap()
        val cachedFog = prepareCoverFogBitmap(solidBitmap(Color.WHITE), Color.MAGENTA)
        cache.putFog(cachedArtwork, cachedFog)
        val uncachedArtwork = solidBitmap(Color.BLACK).asImageBitmap()

        var artwork by mutableStateOf(cachedArtwork)
        val observations = mutableListOf<CoverFogBitmap?>()

        compose.setContent {
            val fog = rememberCoverFogBitmap(
                artwork = artwork,
                fallback = androidx.compose.ui.graphics.Color.Magenta,
                cache = cache,
                preparationDispatcher = dispatcher,
            )
            SideEffect { observations += fog }
        }

        compose.waitForIdle()
        compose.runOnIdle { artwork = uncachedArtwork }
        compose.runOnIdle {
            assertTrue("the replacement blur must be queued", dispatcher.hasWork())
            assertSame(cachedFog, observations.last())
        }

        dispatcher.runAll()
        compose.waitUntil(timeoutMillis = 5_000) { observations.last() !== cachedFog }

        val firstFog = observations.indexOfFirst { it != null }
        check(observations.drop(firstFog).none { it == null }) {
            "the fog must never read null again once the first one has landed"
        }
        assertNotEquals(cachedFog.palette.clouds, observations.last()?.palette?.clouds)
    }

    private fun solidBitmap(color: Int): Bitmap =
        Bitmap.createBitmap(8, 8, Bitmap.Config.ARGB_8888).apply { eraseColor(color) }
}

private class QueuedFogDispatcher : CoroutineDispatcher() {
    private val work = ArrayDeque<Runnable>()

    override fun dispatch(context: CoroutineContext, block: Runnable) {
        work.addLast(block)
    }

    fun hasWork(): Boolean = work.isNotEmpty()

    fun runAll() {
        while (work.isNotEmpty()) work.removeFirst().run()
    }
}
