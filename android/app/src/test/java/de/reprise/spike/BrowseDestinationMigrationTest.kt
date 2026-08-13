package de.reprise.spike

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.reprise_android_ffi.AndroidStoredLibraryDestination

@RunWith(RobolectricTestRunner::class)
@Config(
    sdk = [36],
    qualifiers = "w412dp-h916dp-port",
    application = RemovedDestinationTestApplication::class,
)
class BrowseDestinationMigrationTest {
    @get:Rule
    val compose = createAndroidComposeRule<MainActivity>()

    @After
    fun releaseTheService() {
        (compose.activity.application as RemovedDestinationTestApplication).releaseService()
    }

    @Test
    fun removedAndUnsetDestinationsResolveToTitles() {
        assertEquals(
            BrowseTab.TITLES,
            AndroidStoredLibraryDestination.Unsupported("albums").toBrowseTab(),
        )
        assertEquals(
            BrowseTab.TITLES,
            AndroidStoredLibraryDestination.Unsupported("favourites").toBrowseTab(),
        )
        assertEquals(BrowseTab.TITLES, AndroidStoredLibraryDestination.Unset.toBrowseTab())
    }

    @Test
    fun activityRestoredFromAlbumsShowsTheTitlesContent() {
        compose.onNodeWithTag("library-destination-TITLES").assertIsDisplayed()
        compose.onNodeWithTag("library-page-TITLES").assertIsDisplayed()
        compose.onNodeWithText("Rotation Song 1").assertIsDisplayed()
    }
}

internal class RemovedDestinationTestApplication : ConfigurationTestApplication() {
    override fun mainActivitySurface(): MainActivitySurfaceDependencies =
        super.mainActivitySurface().copy(
            initialStoredDestination = AndroidStoredLibraryDestination.Unsupported("albums"),
        )
}
