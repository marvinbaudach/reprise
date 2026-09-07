package io.github.marvinbaudach.reprise

import android.os.Looper
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
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
    fun searchingTitlesReadsTheLibraryOffTheMainThread() {
        compose.onNodeWithContentDescription("Search library").performClick()
        compose.onNodeWithText("Search titles").performTextInput("rotation")
        compose.waitUntil { application.searchThread.get() != null }

        assertNotSame(Looper.getMainLooper().thread, application.searchThread.get())
    }
}

internal class LibraryReadsOffMainThreadApplication : ConfigurationTestApplication() {
    val searchThread = AtomicReference<Thread>()

    override fun mainActivitySurface(): MainActivitySurfaceDependencies {
        val dependencies = super.mainActivitySurface()
        return dependencies.copy(
            searchTitles = { query, range ->
                if (query.isNotEmpty()) searchThread.set(Thread.currentThread())
                dependencies.searchTitles(query, range)
            },
        )
    }
}
