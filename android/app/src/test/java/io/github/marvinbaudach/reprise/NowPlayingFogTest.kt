package io.github.marvinbaudach.reprise

import io.github.marvinbaudach.reprise.scene.FogDrive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class NowPlayingFogTest {
    /**
     * The title band sits on bare film, so the scrim under it has to cover the
     * rows themselves and stop before the edges of the screen.
     */
    @Test
    fun the_title_scrim_covers_the_title_rows_without_outdarkening_the_edges() {
        assertTrue(
            "the scrim must be solid before the title row at 156 dp",
            NowPlayingFogSpec.titleScrimSolidTopDp <= 156f,
        )
        assertTrue(
            "the scrim must stay solid past the artist row at 246 dp",
            NowPlayingFogSpec.titleScrimSolidBottomDp >= 246f,
        )
        assertTrue(
            "the scrim must fade in before it is solid",
            NowPlayingFogSpec.titleScrimFadeTopDp < NowPlayingFogSpec.titleScrimSolidTopDp,
        )
        assertTrue(
            "the scrim must fade out after it is solid",
            NowPlayingFogSpec.titleScrimFadeBottomDp > NowPlayingFogSpec.titleScrimSolidBottomDp,
        )
        assertTrue(
            "the scrim must stay under the surface edges' own darkening",
            NowPlayingFogSpec.titleScrimAlpha < 0.72f,
        )
    }

    /** The shimmer's reading still lands inside the unit range it promises. */
    @Test
    fun the_shimmer_readings_clamp_instead_of_extrapolating() {
        listOf(-1f, 0f, 0.35f, 1f, 4f).forEach { value ->
            assertTrue(NowPlayingFogSpec.normalizedSwell(value) in 0f..1f)
        }
        assertEquals(0f, NowPlayingFogSpec.normalizedSwell(0f), 0f)
        assertEquals(1f, NowPlayingFogSpec.normalizedSwell(1f), 0f)
    }

    /**
     * The film must not reach for a platform blur, whatever it is asked to look
     * like.
     *
     * The design behind this layer filters the whole cloud container through
     * `blur(34px) saturate(1.6) contrast(1.2)`. Both halves of that are
     * `RenderEffect` on Android and `RenderEffect` is API 31, four releases past
     * this app's floor — so the blur is answered by the gradients' own
     * smoothstepped falloff and the grade is baked into the palette. This reads
     * the sources back to make sure a later hand does not quietly reach for the
     * modifier that would have been easier.
     */
    @Test
    fun the_oil_film_never_depends_on_api_31_blur() {
        val source = listOf(
            "NowPlayingFog.kt",
            "NowPlayingOilFilm.kt",
            "OilFilmPalette.kt",
            "CoverFogBitmap.kt",
        ).joinToString("\n") { name ->
            // Comments are stripped first, so the files stay free to name the
            // API they are avoiding and to say why they are avoiding it.
            withoutComments(java.io.File("src/main/java/io/github/marvinbaudach/reprise/$name").readText())
        }

        assertTrue("Modifier.blur must not enter the Now Playing fog", "Modifier.blur" !in source)
        assertTrue("RenderEffect would violate minSdk 26", "RenderEffect" !in source)
    }

    private fun withoutComments(source: String): String =
        source.replace(BLOCK_COMMENT, "").replace(LINE_COMMENT, "")

    private companion object {
        val BLOCK_COMMENT = Regex("/\\*.*?\\*/", RegexOption.DOT_MATCHES_ALL)
        val LINE_COMMENT = Regex("//[^\\n]*")
    }
}
