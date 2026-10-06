package io.github.marvinbaudach.reprise.library

import android.net.Uri
import java.util.concurrent.Executor
import java.util.concurrent.RejectedExecutionException
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class CurrentTrackArtworkTest {
    private val queued = ArrayDeque<Runnable>()
    private val executor = Executor { queued.addLast(it) }
    private val resolved = mutableListOf<String>()
    private val attached = mutableListOf<Pair<String, Uri>>()
    private var answer: (String) -> Uri? = { Uri.parse("file:///cache/${it.substringAfterLast('/')}.png") }
    private val artwork = CurrentTrackArtwork(
        executor = executor,
        resolve = { uri ->
            resolved += uri
            answer(uri)
        },
        attach = { uri, cover -> attached += uri to cover },
    )

    private fun runQueued() {
        while (queued.isNotEmpty()) queued.removeFirst().run()
    }

    @Test
    fun aNewCurrentTrackGetsItsCoverAttachedToItsOwnUri() {
        artwork.onCurrentTrack("content://tree/1.flac")
        runQueued()

        assertEquals(
            listOf("content://tree/1.flac" to Uri.parse("file:///cache/1.flac.png")),
            attached,
        )
    }

    @Test
    fun aSnapshotThatArrivesAfterTheExecutorShutDownIsDroppedNotThrown() {
        val closed = CurrentTrackArtwork(
            executor = { throw RejectedExecutionException("shut down") },
            resolve = { null },
            attach = { _, _ -> },
        )

        closed.onCurrentTrack("content://tree/1.flac")
    }

    // Fetching once is right because the player remembers the cover per uri
    // (`Media3PlaybackPortMetadataTest.aTrackPlayedAgainStartsWithItsCoverAndAsksNothing`):
    // a replay builds its item with the cover instead of asking again.
    @Test
    fun theSameTrackIsFetchedOnceHoweverOftenPlaybackChanges() {
        repeat(5) { artwork.onCurrentTrack("content://tree/1.flac") }
        runQueued()

        assertEquals(1, resolved.size)
    }

    @Test
    fun noTrackStartsNothing() {
        artwork.onCurrentTrack(null)
        runQueued()

        assertEquals(emptyList<String>(), resolved)
    }

    @Test
    fun aTrackWithoutACoverAttachesNothing() {
        answer = { null }

        artwork.onCurrentTrack("content://tree/2.flac")
        runQueued()

        assertEquals(emptyList<Pair<String, Uri>>(), attached)
    }

    @Test
    fun aFailingLookupIsContainedAndTheNextTrackStillWorks() {
        answer = { error("document provider went away") }
        artwork.onCurrentTrack("content://tree/3.flac")
        runQueued()
        answer = { Uri.parse("file:///cache/ok.png") }

        artwork.onCurrentTrack("content://tree/4.flac")
        runQueued()

        assertEquals(listOf("content://tree/4.flac" to Uri.parse("file:///cache/ok.png")), attached)
    }
}
