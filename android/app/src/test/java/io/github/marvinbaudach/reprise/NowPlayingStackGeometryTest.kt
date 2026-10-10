package io.github.marvinbaudach.reprise

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The stacked Now Playing scene lays its blocks out by their heights (#1190).
 * A screen with room keeps the fractions the scene has always used; a screen
 * without gives way from the bottom up, and text never shrinks to make room.
 */
class NowPlayingStackGeometryTest {
    @Test
    fun aTallScreenKeepsTheFractionalPositions() {
        val geometry = nowPlayingStackGeometry(inputs(heightDp = 874f))

        assertEquals(874f * 0.34f, geometry.coverCentreYDp, 0.01f)
        assertEquals(1f, geometry.coverScale, 0f)
        assertEquals(874f * 0.34f + 156f, geometry.titleTopDp, 0.01f)
        assertEquals(874f * 0.69f, geometry.seekTopDp, 0.01f)
        assertEquals(2, geometry.titleMaxLines)
    }

    @Test
    fun aShortScreenLiftsTheSeekBlockClearOfTheSideButtons() {
        val geometry = nowPlayingStackGeometry(inputs(heightDp = 598f, seekLabelDp = 32f))

        val sideButtonsTop = 598f - NAVIGATION_INSET_DP - 18f - 80f + 16f
        assertTrue(geometry.seekTopDp + 48f + 32f <= sideButtonsTop)
    }

    @Test
    fun aShortScreenKeepsEveryBlockAboveTheNextOne() {
        val geometry = nowPlayingStackGeometry(
            inputs(heightDp = 598f, titleLineDp = 43f, artistLineDp = 29f, seekLabelDp = 32f),
        )

        val titleBlock = geometry.titleMaxLines * 43f + 6f + 29f
        assertTrue(geometry.titleTopDp + titleBlock <= geometry.seekTopDp)
        val coverBottom = geometry.coverCentreYDp + 272f * geometry.coverScale / 2f
        assertTrue(coverBottom <= geometry.titleTopDp)
    }

    @Test
    fun theCoverGivesFirstAndTheTitleDropsToOneLineOnlyBelowTheFloor() {
        val relaxed = nowPlayingStackGeometry(inputs(heightDp = 598f))
        val tight = nowPlayingStackGeometry(
            inputs(heightDp = 560f, titleLineDp = 43f, artistLineDp = 29f, seekLabelDp = 32f),
        )

        assertEquals(2, relaxed.titleMaxLines)
        assertTrue(relaxed.coverScale < 1f && relaxed.coverScale >= 0.7f)
        assertEquals(1, tight.titleMaxLines)
        assertTrue(tight.coverScale >= 0.7f)
    }

    @Test
    fun theCoverNeverRisesIntoTheHeader() {
        val geometry = nowPlayingStackGeometry(inputs(heightDp = 598f, seekLabelDp = 32f))

        val coverTop = geometry.coverCentreYDp - 272f * geometry.coverScale / 2f
        assertTrue("cover top $coverTop", coverTop >= 56f)
    }

    @Test
    fun aScreenTooShortForAnyLayoutStillReturnsASaneCover() {
        val geometry = nowPlayingStackGeometry(inputs(heightDp = 300f, seekLabelDp = 32f))

        assertTrue(geometry.coverScale >= 0.5f && geometry.coverScale <= 1f)
        assertTrue(geometry.coverCentreYDp > 0f)
    }

    private fun inputs(
        heightDp: Float,
        titleLineDp: Float = 29f,
        artistLineDp: Float = 16f,
        seekLabelDp: Float = 16f,
    ) = NowPlayingStackInputs(
        heightDp = heightDp,
        navigationInsetDp = NAVIGATION_INSET_DP,
        titleLineDp = titleLineDp,
        artistLineDp = artistLineDp,
        seekLabelDp = seekLabelDp,
    )

    private companion object {
        const val NAVIGATION_INSET_DP = 24f
    }
}
