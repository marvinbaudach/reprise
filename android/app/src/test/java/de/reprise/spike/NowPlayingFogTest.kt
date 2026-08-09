package de.reprise.spike

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class NowPlayingFogTest {
    @Test
    fun fog_layers_keep_the_specified_geometry_blend_and_counter_rotation() {
        assertEquals(620f, NowPlayingFogSpec.wideSizeDp)
        assertEquals(470f, NowPlayingFogSpec.tightSizeDp)
        assertEquals(0.92f, NowPlayingFogSpec.wideOpacity)
        assertEquals(0.55f, NowPlayingFogSpec.tightOpacity)
        assertEquals(0.9f, NowPlayingFogSpec.wideAngleFactor)
        assertEquals(-0.6f, NowPlayingFogSpec.tightAngleFactor)
        assertTrue(NowPlayingFogSpec.tightUsesScreenBlend)
    }

    @Test
    fun the_title_scrim_covers_the_title_rows_without_outdarkening_the_edges() {
        // The rows the scene actually draws: title block top is 156 dp under
        // the cover centre, a two-line title plus the artist line ends by 246.
        assertTrue(
            "the scrim must be at full strength before the title starts",
            NowPlayingFogSpec.titleScrimSolidTopDp <= 156f,
        )
        assertTrue(
            "the scrim must still be at full strength where the artist line ends",
            NowPlayingFogSpec.titleScrimSolidBottomDp >= 246f,
        )
        assertTrue(
            "the scrim needs room to fade in above the rows",
            NowPlayingFogSpec.titleScrimFadeTopDp < NowPlayingFogSpec.titleScrimSolidTopDp,
        )
        assertTrue(
            "the scrim needs room to fade out below the rows",
            NowPlayingFogSpec.titleScrimFadeBottomDp > NowPlayingFogSpec.titleScrimSolidBottomDp,
        )
        // A dark cover must not end up with the middle of the screen darker
        // than its own edges, which would read as a hole rather than a scrim.
        assertTrue(
            "the title scrim must stay lighter than the top and bottom scrims",
            NowPlayingFogSpec.titleScrimAlpha < 0.72f,
        )
    }

    @Test
    fun cover_fog_preparation_never_depends_on_api_31_blur() {
        val source = java.io.File(
            "src/main/java/de/reprise/spike/NowPlayingFog.kt",
        ).readText() + java.io.File(
            "src/main/java/de/reprise/spike/CoverFogBitmap.kt",
        ).readText()

        assertTrue("Modifier.blur must not enter the Now Playing fog", "Modifier.blur" !in source)
        assertTrue("RenderEffect would violate minSdk 26", "RenderEffect" !in source)
    }
}
