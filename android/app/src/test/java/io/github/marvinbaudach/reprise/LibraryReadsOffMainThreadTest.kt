package io.github.marvinbaudach.reprise

import android.os.Looper
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithContentDescription
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextInput
import java.util.concurrent.atomic.AtomicReference
import org.junit.After
import org.junit.Assert.assertNotSame
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(
    sdk = [36],
    qualifiers = "w412dp-h916dp-port",
    application = LibraryReadsOffMainThreadApplication::class,
)
class LibraryReadsOffMainThreadTest {
    @get:Rule
    val compose = createAndroidComposeRule<MainActivity>()

    private val application: LibraryReadsOffMainThreadApplication
        get() = RuntimeEnvironment.getApplication() as LibraryReadsOffMainThreadApplication

    @After
    fun releaseTheService() {
        application.releaseService()
    }

    @Test
    fun libraryReadsRunOffTheMainThread() {
        compose.onNodeWithText("Artists").performClick()
        compose.waitUntil(timeoutMillis = 5_000) { application.artistListThread.get() != null }
        compose.waitUntil(timeoutMillis = 5_000) {
            compose.onAllNodesWithText("Artist 1").fetchSemanticsNodes().isNotEmpty()
        }
        compose.onAllNodesWithText("Artist 1")[0].performClick()
        compose.waitUntil(timeoutMillis = 5_000) {
            compose.onAllNodesWithText("First Album").fetchSemanticsNodes().isNotEmpty()
        }
        compose.onNodeWithText("First Album").performClick()
        compose.waitUntil(timeoutMillis = 5_000) { application.albumOpenThread.get() != null }
        compose.waitUntil(timeoutMillis = 5_000) {
            compose.onAllNodesWithContentDescription("Back").fetchSemanticsNodes().isNotEmpty()
        }

        compose.onNodeWithContentDescription("Back").performClick()
        compose.onNodeWithContentDescription("Back to artists").performClick()
        compose.onNodeWithText("Titles").performClick()
        compose.onNodeWithContentDescription("Search library").performClick()
        compose.onNodeWithText("Search titles").performTextInput("rotation")
        compose.waitUntil(timeoutMillis = 5_000) { application.searchThread.get() != null }

        val mainThread = Looper.getMainLooper().thread
        assertNotSame(mainThread, application.artistListThread.get())
        assertNotSame(mainThread, application.albumOpenThread.get())
        assertNotSame(mainThread, application.searchThread.get())
    }
}

internal class LibraryReadsOffMainThreadApplication : ConfigurationTestApplication() {
    val searchThread = AtomicReference<Thread>()
    val artistListThread = AtomicReference<Thread>()
    val albumOpenThread = AtomicReference<Thread>()

    override fun mainActivitySurface(): MainActivitySurfaceDependencies {
        val dependencies = super.mainActivitySurface()
        return dependencies.copy(
            listArtists = { range ->
                artistListThread.set(Thread.currentThread())
                dependencies.listArtists(range)
            },
            openAlbum = { album ->
                albumOpenThread.set(Thread.currentThread())
                dependencies.openAlbum(album)
            },
            searchTitles = { query, range ->
                if (query.isNotEmpty()) searchThread.set(Thread.currentThread())
                dependencies.searchTitles(query, range)
            },
        )
    }
}
