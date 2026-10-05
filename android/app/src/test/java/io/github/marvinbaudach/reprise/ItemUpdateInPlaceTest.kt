package io.github.marvinbaudach.reprise

import android.net.Uri
import androidx.annotation.OptIn
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import androidx.media3.common.util.UnstableApi
import androidx.media3.exoplayer.source.DefaultMediaSourceFactory
import androidx.test.core.app.ApplicationProvider
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * A late cover is attached by replacing the playing item with one that only
 * differs in its metadata. That is cheap only if ExoPlayer can update the item
 * in place; if it could not, every track would restart when its cover arrived.
 */
@OptIn(UnstableApi::class)
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class ItemUpdateInPlaceTest {
    @Test
    fun anItemThatGainsACoverCanBeUpdatedWithoutRebuildingItsSource() {
        val uri = Uri.parse("content://tree/1.flac")
        val before = MediaItem.Builder().setUri(uri).setMediaId("1")
            .setMediaMetadata(MediaMetadata.Builder().setTitle("Song").build())
            .build()
        val after = before.buildUpon()
            .setMediaMetadata(
                before.mediaMetadata.buildUpon().setArtworkUri(Uri.parse("file:///cache/1.png")).build(),
            )
            .build()
        val source = DefaultMediaSourceFactory(ApplicationProvider.getApplicationContext<android.content.Context>())
            .createMediaSource(before)

        assertTrue(source.canUpdateMediaItem(after))
    }
}
