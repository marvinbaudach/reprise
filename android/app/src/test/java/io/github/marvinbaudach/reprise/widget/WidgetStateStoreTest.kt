package io.github.marvinbaudach.reprise.widget

import android.content.Context
import androidx.test.core.app.ApplicationProvider
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class WidgetStateStoreTest {
    private val context: Context = ApplicationProvider.getApplicationContext()
    private val state = WidgetNowPlaying(9, "Song", "Singer", isPlaying = true, artworkPath = "/cache/9.png")

    @After
    fun forgetTheProcess() {
        WidgetStateStore.resetProcessState()
    }

    @Test
    fun aNeverUsedStoreIsEmpty() {
        assertTrue(WidgetStateStore(context).load().isEmpty)
    }

    @Test
    fun theLastTrackSurvivesANewStoreInstance() {
        WidgetStateStore(context).save(state)

        val loaded = WidgetStateStore(context).load()

        assertEquals(state, loaded)
    }

    @Test
    fun aRestartedProcessDoesNotClaimToBePlaying() {
        WidgetStateStore(context).save(state)

        WidgetStateStore.resetProcessState()

        assertEquals(state.copy(isPlaying = false), WidgetStateStore(context).load())
    }

    @Test
    fun savingTheEmptyStateForgetsTheTrack() {
        val store = WidgetStateStore(context)
        store.save(state)

        store.save(WidgetNowPlaying.Empty)

        assertTrue(store.load().isEmpty)
    }

    @Test
    fun whetherThereIsAQueueToResumeSurvivesTheProcess() {
        WidgetStateStore(context).save(state.copy(isPlaying = false, canResume = false))

        assertEquals(false, WidgetStateStore(context).load().canResume)
    }
}
