package io.github.marvinbaudach.reprise

import android.graphics.Bitmap
import androidx.activity.ComponentActivity
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import org.junit.Assert.assertSame
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.reprise_android_ffi.AndroidArtworkSize

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class ArtworkCompositionTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    fun cached_cover_is_non_null_in_the_first_composition() {
        val cache = ArtworkCache()
        val request = ArtworkRequest(
            trackUri = "content://tracks/cached-composition",
            size = AndroidArtworkSize.NOW_PLAYING,
            title = "Cached",
            artist = "Artist",
        )
        val expected = ArtworkVisual(
            Bitmap.createBitmap(4, 4, Bitmap.Config.ARGB_8888).asImageBitmap(),
            ambientColors = null,
        )
        cache.putArtwork(request, expected)
        val artwork = TrackArtwork(
            resolve = { _, _ -> error("cached artwork must not resolve again") },
            cache = cache,
        )
        var first: ArtworkVisual? = null

        try {
            compose.setContent {
                CompositionLocalProvider(LocalTrackArtwork provides artwork) {
                    first = rememberTrackArtworkVisual(
                        request.trackUri,
                        request.size,
                        request.title,
                        request.artist,
                    )
                }
            }
            compose.runOnIdle { assertSame(expected, first) }
        } finally {
            artwork.shutdown()
        }
    }
}
