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
