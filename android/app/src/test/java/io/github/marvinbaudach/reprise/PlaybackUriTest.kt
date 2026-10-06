package io.github.marvinbaudach.reprise

import android.net.Uri
import androidx.media3.common.Player
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class PlaybackUriTest {
    private val colonPath = "/storage/emulated/0/Music/AC:DC/x.mp3"

    @Test
    fun aLocalPathWithAColonStaysAFilePath() {
        val uri = playbackUri(colonPath)

        assertEquals("file", uri.scheme)
        assertEquals(colonPath, uri.path)
        assertEquals(Uri.fromFile(File(colonPath)), uri)
    }

    @Test
    fun aRealSchemeIsKept() {
        assertEquals("content", playbackUri("content://media/external/audio/media/7").scheme)
        assertEquals("https", playbackUri("https://example.org/a.mp3").scheme)
        assertEquals("file", playbackUri("file:///music/a.mp3").scheme)
    }

    @Test
    fun aPathWithoutALeadingSlashOrASchemeIsStillAFile() {
        val uri = playbackUri("AC:DC/x.mp3")

        assertEquals("file", uri.scheme)
        assertEquals(File("AC:DC/x.mp3").absolutePath, uri.path)
    }

    @Test
    fun theCurrentAndTheGaplessNextItemBothSurviveAColonInThePath() {
        val fake = CallbackPlayer(playbackState = Player.STATE_IDLE, playWhenReady = false)
        val port = Media3PlaybackPort(fake.player) {}

        port.setNext("/storage/emulated/0/Music/AC:DC/y.mp3", 0.0)
        port.playPath(colonPath, 0.0)

        assertEquals(
            listOf(colonPath, "/storage/emulated/0/Music/AC:DC/y.mp3"),
            fake.mediaItems.map { it.localConfiguration!!.uri.path },
        )
        assertEquals(
            listOf("file", "file"),
            fake.mediaItems.map { it.localConfiguration!!.uri.scheme },
        )

        port.setNext("/storage/emulated/0/Music/Q:Q/z.mp3", 0.0)

        assertEquals("file", fake.mediaItems.last().localConfiguration!!.uri.scheme)
        assertEquals(
            "/storage/emulated/0/Music/Q:Q/z.mp3",
            fake.mediaItems.last().localConfiguration!!.uri.path,
        )
        port.release()
    }
}
