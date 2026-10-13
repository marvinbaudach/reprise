package io.github.marvinbaudach.reprise

import android.content.Intent
import android.graphics.Bitmap
import android.os.Looper
import androidx.activity.ComponentActivity
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performSemanticsAction
import androidx.compose.ui.test.performScrollToIndex
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.click
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.height
import io.github.marvinbaudach.reprise.ui.theme.RepriseTheme
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import uniffi.reprise_android_ffi.AndroidArtworkSize
import uniffi.reprise_android_ffi.AndroidColorScheme
import uniffi.reprise_android_ffi.AndroidPortraitCredit
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36], qualifiers = "w412dp-h916dp-port")
class ArtistDetailHeadTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    fun art_3_thePlayButtonScrollsAwayWithThePortrait() {
        showLongArtistFromTheList()

        compose.onNodeWithText(ARTIST_NAME).performClick()
        compose.onNodeWithTag("library-artist-albums-list").performScrollToIndex(12)

        compose.onNodeWithTag("artist-detail-play").assertDoesNotExist()
        compose.onNodeWithTag("artist-portrait-head-image", useUnmergedTree = true)
            .assertDoesNotExist()
        compose.onNodeWithContentDescription("Back to artists").assertIsDisplayed()
    }

    @Test
    fun art_3_playStillStartsAfterTheHeadScrollsAway() {
        val lookupStarted = CountDownLatch(1)
        val releaseLookup = CountDownLatch(1)
        val playCount = AtomicInteger()
        val controls = object : PlaybackControls by DisconnectedPlaybackControls {
            override fun playTrackIds(trackIds: List<Long>, startIndex: Int) {
                playCount.incrementAndGet()
            }
        }
        showLongArtistFromTheList(
            artistTrackIds = {
                lookupStarted.countDown()
                check(releaseLookup.await(5, TimeUnit.SECONDS))
                listOf(41L)
            },
            controls = controls,
        )

        compose.onNodeWithText(ARTIST_NAME).performClick()
        compose.onNodeWithTag("artist-detail-play").performClick()
        assertTrue(lookupStarted.await(5, TimeUnit.SECONDS))
        compose.onNodeWithTag("library-artist-albums-list").performScrollToIndex(12)
        compose.onNodeWithTag("artist-detail-play").assertDoesNotExist()
        releaseLookup.countDown()

        compose.waitUntil(5_000) { playCount.get() == 1 }
    }

    @Test
    fun art_3_aSmallScrollMovesThePlayButtonAndPortraitByTheSameDelta() {
        showLongArtistFromTheList()
        compose.onNodeWithText(ARTIST_NAME).performClick()
        val play = compose.onNodeWithTag("artist-detail-play")
        val portrait = compose.onNodeWithTag("artist-portrait-head-image", useUnmergedTree = true)
        val list = compose.onNodeWithTag("library-artist-albums-list")
        val playTop = play.getUnclippedBoundsInRoot().top.value
        val portraitTop = portrait.getUnclippedBoundsInRoot().top.value

        list.performSemanticsAction(SemanticsActions.ScrollBy) { scroll -> scroll(0f, 12f) }
        compose.waitForIdle()

        val playDelta = playTop - play.getUnclippedBoundsInRoot().top.value
        val portraitDelta = portraitTop - portrait.getUnclippedBoundsInRoot().top.value
        assertTrue(playDelta > 0f)
        assertEquals(playDelta, portraitDelta, 0.1f)
    }

    @Test
    fun art_3_theTitleRowTakesItsContainerOnlyOnceTheListScrolls() {
        showLongArtistFromTheList()
        compose.onNodeWithText(ARTIST_NAME).performClick()
        val titleRow = compose.onNodeWithTag("artist-detail-header")
        val statusInset = compose.onNodeWithTag("artist-detail-status-inset")
        val list = compose.onNodeWithTag("library-artist-albums-list")

        titleRow.assert(SemanticsMatcher.expectValue(ArtistHeaderScrolledKey, false))
        titleRow.assert(SemanticsMatcher.expectValue(SemanticsProperties.IsTraversalGroup, true))
        titleRow.assert(SemanticsMatcher.expectValue(SemanticsProperties.TraversalIndex, -1f))
        assertTrue(
            statusInset.getUnclippedBoundsInRoot().bottom >
                titleRow.getUnclippedBoundsInRoot().bottom,
        )
        list.performScrollToIndex(12)
        titleRow.assert(SemanticsMatcher.expectValue(ArtistHeaderScrolledKey, true))
        assertEquals(
            titleRow.getUnclippedBoundsInRoot().bottom,
            statusInset.getUnclippedBoundsInRoot().bottom,
        )
        list.performScrollToIndex(0)
        titleRow.assert(SemanticsMatcher.expectValue(ArtistHeaderScrolledKey, false))
    }

    @Test
    fun art_3_theOpaqueTitleRowConsumesTapsInItsGaps() {
        val tapsBehind = AtomicInteger()
        val scrolled = mutableStateOf(false)
        compose.setContent {
            RepriseTheme(THEME, darkPalette = true) {
                Box(modifier = androidx.compose.ui.Modifier.fillMaxSize()) {
                    Box(
                        modifier = androidx.compose.ui.Modifier
                            .fillMaxSize()
                            .clickable { tapsBehind.incrementAndGet() },
                    )
                    ArtistDetailTitleRow(
                        artist = LibraryArtist(ARTIST_NAME, 2, 1, "content://artist"),
                        closeArtist = {},
                        scrolled = scrolled.value,
                        withMenu = false,
                    ) {
                        Spacer(
                            modifier = androidx.compose.ui.Modifier
                                .fillMaxWidth()
                                .height(24.dp)
                                .testTag("artist-title-gap"),
                        )
                    }
                }
            }
        }

        compose.onNodeWithTag("artist-title-gap").performTouchInput { click(center) }
        assertEquals(1, tapsBehind.get())
        compose.runOnIdle { scrolled.value = true }
        compose.onNodeWithTag("artist-title-gap").performTouchInput { click(center) }

        assertEquals(1, tapsBehind.get())
    }

    @Test
    fun art_4_aCommonsPortraitShowsItsCreditBeneathIt() {
        showLongArtistFromTheList(credits = mapOf(ARTIST_NAME to commonsCredit()))

        compose.onNodeWithText(ARTIST_NAME).performClick()
        val credit = compose.onNodeWithText(COMMONS_CREDIT_TEXT)
        compose.waitUntil(5_000) { creditNodes(COMMONS_CREDIT_TEXT) == 1 }

        credit.assertIsDisplayed()
        credit.assert(SemanticsMatcher.expectValue(SemanticsProperties.Role, Role.Button))
        credit.assert(
            SemanticsMatcher("clicks with the label Open image source") { node ->
                node.config.getOrNull(SemanticsActions.OnClick)?.label == "Open image source"
            },
        )
        credit.performClick()
        val launched = shadowOf(compose.activity).nextStartedActivity
        assertEquals(Intent.ACTION_VIEW, launched.action)
        assertEquals(COMMONS_FILE_PAGE, launched.dataString)
    }

    @Test
    fun art_4_theCreditHasATouchTargetOfAtLeast48dp() {
        showLongArtistFromTheList(credits = mapOf(ARTIST_NAME to commonsCredit()))

        compose.onNodeWithText(ARTIST_NAME).performClick()
        compose.waitUntil(5_000) { creditNodes(COMMONS_CREDIT_TEXT) == 1 }

        val bounds = compose.onNodeWithTag("artist-portrait-credit").getUnclippedBoundsInRoot()
        assertTrue(bounds.height >= 48.dp)
    }

    @Test
    fun art_4_aDeezerPortraitShowsNoCredit() {
        val session = showLongArtistFromTheList(credits = mapOf(ARTIST_NAME to null))

        compose.onNodeWithText(ARTIST_NAME).performClick()
        awaitCreditLookup(session, ARTIST_NAME)

        compose.onNodeWithTag("artist-portrait-credit").assertDoesNotExist()
        compose.onNodeWithText("Wikimedia Commons", substring = true).assertDoesNotExist()
    }

    @Test
    fun art_4_aCreditBeneathAGeneratedCoverIsNotShown() {
        val session = showLongArtistFromTheList(
            credits = mapOf(ARTIST_NAME to commonsCredit()),
            portraitDecodes = false,
        )

        compose.onNodeWithText(ARTIST_NAME).performClick()
        awaitCreditLookup(session, ARTIST_NAME)

        compose.onNodeWithTag("artist-portrait-credit").assertDoesNotExist()
    }

    @Test
    fun art_4_aCreditDoesNotFollowTheReaderToTheNextArtist() {
        val session = showLongArtistFromTheList(
            credits = mapOf(ARTIST_NAME to commonsCredit(), OTHER_ARTIST_NAME to null),
        )

        compose.onNodeWithText(ARTIST_NAME).performClick()
        compose.waitUntil(5_000) { creditNodes(COMMONS_CREDIT_TEXT) == 1 }
        compose.onNodeWithContentDescription("Back to artists").performClick()
        compose.onNodeWithText(OTHER_ARTIST_NAME).performClick()
        awaitCreditLookup(session, OTHER_ARTIST_NAME)

        compose.onNodeWithTag("artist-portrait-credit").assertDoesNotExist()
    }

    @Test
    fun artistPortraitCreditRefusesEverythingButACommonsFilePageOverHttps() {
        assertTrue(isHttpsCreditUrl(COMMONS_FILE_PAGE))
        assertTrue(isHttpsCreditUrl("https://Commons.Wikimedia.org/wiki/File:Portrait.jpg"))
        assertTrue(!isHttpsCreditUrl("http://commons.wikimedia.org/wiki/File:Portrait.jpg"))
        assertTrue(!isHttpsCreditUrl("javascript:alert(1)"))
        assertTrue(!isHttpsCreditUrl("https:commons.wikimedia.org/wiki/File:Portrait.jpg"))
        assertTrue(!isHttpsCreditUrl("https://evil.example/x"))
        assertTrue(!isHttpsCreditUrl("https://commons.wikimedia.org.evil.example/"))
        assertTrue(!isHttpsCreditUrl("https://commons.wikimedia.org@evil.example/"))
        assertTrue(!isHttpsCreditUrl("https://user:secret@commons.wikimedia.org/wiki/File:A.jpg"))
    }

    private fun commonsCredit() = AndroidPortraitCredit(
        author = "Jane Photographer",
        license = "CC BY-SA 4.0",
        licenseUrl = "https://creativecommons.org/licenses/by-sa/4.0/",
        filePageUrl = COMMONS_FILE_PAGE,
    )

    private fun creditNodes(text: String): Int =
        compose.onAllNodesWithText(text).fetchSemanticsNodes().size

    /** Waits until the session has finished looking the artist's credit up, found or not. */
    private fun awaitCreditLookup(session: LibrarySession, name: String) {
        compose.waitUntil(5_000) { session.artistPortraitCredits.value.containsKey(name) }
        compose.waitForIdle()
    }

    private fun showLongArtistFromTheList(
        artistTrackIds: (LibraryArtist) -> List<Long> = { emptyList() },
        controls: PlaybackControls = DisconnectedPlaybackControls,
        credits: Map<String, AndroidPortraitCredit?> = emptyMap(),
        portraitDecodes: Boolean = true,
    ): LibrarySession {
        val artists = listOf(ARTIST_NAME, OTHER_ARTIST_NAME).map(::artistNamed)
        val details = artists.associateBy(LibraryArtist::name) { artist ->
            ArtistTrackList(
                artist = artist,
                albums = window(albumsOf(artist.name)),
                untaggedTracks = LibraryWindow.empty(),
            )
        }
        val surfaceState = MobileSurfaceViewModel()
        val session = LibrarySession(
            fakeLibrarySessionPort(
                artistPortraitFetched = { _, _ -> "/portraits/portrait.png" },
                artistPortraitCredit = { name ->
                    check(Looper.myLooper() != Looper.getMainLooper())
                    credits[name]
                },
            ),
        )
        val artwork = TrackArtwork(
            resolve = session::artworkFor,
            resolveArtistPortraitFetched = session::artistPortraitFetched,
            decode = { _ ->
                if (portraitDecodes) Bitmap.createBitmap(1, 1, Bitmap.Config.ARGB_8888) else null
            },
            fallback = { _, _, _ -> Bitmap.createBitmap(1, 1, Bitmap.Config.ARGB_8888) },
            cache = ArtworkCache(),
        )
        compose.setContent {
            RepriseTheme(THEME, darkPalette = true) {
                CompositionLocalProvider(
                    LocalArtistTrackIds provides artistTrackIds,
                    LocalPlaybackControls provides controls,
                    LocalTrackArtwork provides artwork,
                    LocalArtistPortraitCredits provides session.artistPortraitCredits,
                ) {
                    var selected by remember { mutableStateOf<ArtistTrackList?>(null) }
                    ArtistsTab(
                        surfaceLayout = SurfaceLayout.STACKED,
                        surfaceState = surfaceState,
                        artists = window(artists),
                        searchText = "",
                        selectedArtist = selected,
                        playback = PlaybackUiState().libraryPlayback(),
                        openArtist = { opened -> selected = details[opened.name] },
                        closeArtist = { selected = null },
                        play = {},
                        lastRequestedOffset = null,
                        artistRequestedOffset = null,
                        loadMoreArtists = {},
                        loadMoreArtistTracks = {},
                    )
                }
            }
        }
        return session
    }

    private fun albumsOf(artistName: String) = (1..20).map { index ->
        LibraryAlbum(
            title = "Album $index",
            artist = artistName,
            representativeUri = "content://albums/$index",
            trackCount = 2,
            year = 2026 - index,
            totalDurationMs = 120_000,
        )
    }

    private fun artistNamed(name: String) = LibraryArtist(
        name = name,
        trackCount = 40,
        albumCount = 20,
        representativeUri = "content://artists/$name",
    )

    private fun <T> window(rows: List<T>) = LibraryWindow(
        total = rows.size.toLong(),
        rows = rows,
        hasMore = false,
    )

    private companion object {
        const val ARTIST_NAME = "Scrollable Artist"
        const val OTHER_ARTIST_NAME = "Another Artist"
        const val COMMONS_FILE_PAGE = "https://commons.wikimedia.org/wiki/File:Portrait.jpg"
        const val COMMONS_CREDIT_TEXT =
            "Photo: Jane Photographer · CC BY-SA 4.0 · Wikimedia Commons"
        val THEME = MobileThemeSelection(
            palette = MobileTheme.NOCTURNE,
            colorScheme = AndroidColorScheme.SYSTEM,
            dynamicAvailable = false,
        )
    }
}

class ArtistHeadShimmerTransformTest {
    @Test
    fun art_3_thePortraitShimmerFollowsAndFadesWithTheHead() {
        assertEquals(
            ArtistHeadShimmerTransform(translationY = 0f, alpha = 1f),
            artistHeadShimmerTransform(firstVisibleIndex = 0, offsetPx = 0, headHeightPx = 300),
        )
        assertEquals(
            ArtistHeadShimmerTransform(translationY = -75f, alpha = 0.75f),
            artistHeadShimmerTransform(firstVisibleIndex = 0, offsetPx = 75, headHeightPx = 300),
        )
        assertEquals(
            0f,
            artistHeadShimmerTransform(
                firstVisibleIndex = 1,
                offsetPx = 0,
                headHeightPx = 300,
            ).alpha,
        )
    }
}

class ArtistPortraitCreditSessionTest {
    @Test
    fun art_4_creditsBelongToTheSessionThatFetchedThem() {
        val credit = AndroidPortraitCredit(
            author = "Jane Photographer",
            license = "CC BY-SA 4.0",
            licenseUrl = "https://creativecommons.org/licenses/by-sa/4.0/",
            filePageUrl = "https://commons.wikimedia.org/wiki/File:Portrait.jpg",
        )
        val fetching = LibrarySession(
            fakeLibrarySessionPort(artistPortraitCredit = { credit }),
        )
        val bystander = LibrarySession(fakeLibrarySessionPort())

        fetching.artistPortraitFetched(" Some Artist ", AndroidArtworkSize.LIST)

        assertEquals(mapOf("Some Artist" to credit), fetching.artistPortraitCredits.value)
        assertTrue(bystander.artistPortraitCredits.value.isEmpty())
    }

    @Test
    fun art_4_aMissingCreditStillReturnsThePortraitPath() {
        val session = LibrarySession(
            fakeLibrarySessionPort(artistPortraitFetched = { _, _ -> "/portraits/a.png" }),
        )

        assertEquals(
            "/portraits/a.png",
            session.artistPortraitFetched("Some Artist", AndroidArtworkSize.LIST),
        )
        assertTrue(session.artistPortraitCredits.value.getValue("Some Artist") == null)
    }
}
