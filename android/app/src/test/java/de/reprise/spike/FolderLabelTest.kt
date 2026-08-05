package de.reprise.spike

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * The settings screen has to say *which* folder it scans. It may only say it
 * when it knows — the alternative is printing a provider's internal token at a
 * listener, which is worse than the generic line it replaces.
 *
 * Robolectric, because `DocumentsContract.getTreeDocumentId` is the thing under
 * test: hand-parsing the URI here would test the parser and not the contract.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class FolderLabelTest {
    @Test
    fun namesTheFolderOutOfAStorageTreeUri() {
        assertEquals(
            "Music",
            folderLabel("content://com.android.externalstorage.documents/tree/primary%3AMusic"),
        )
    }

    /**
     * The whole relative path, not its last segment: `Live` alone is exactly
     * what a listener with `Music/Live` and `Podcasts/Live` cannot tell apart.
     */
    @Test
    fun keepsTheWholePathOfANestedFolder() {
        assertEquals(
            "Music/Live",
            folderLabel(
                "content://com.android.externalstorage.documents/tree/primary%3AMusic%2FLive",
            ),
        )
    }

    @Test
    fun saysNothingAboutAVolumeRoot() {
        assertNull(folderLabel("content://com.android.externalstorage.documents/tree/primary%3A"))
    }

    @Test
    fun saysNothingWhenTheProvidersIdIsAnOpaqueToken() {
        assertNull(
            folderLabel("content://com.example.cloud.documents/tree/8f14e45fceea167a"),
        )
    }

    @Test
    fun saysNothingWithoutAFolder() {
        assertNull(folderLabel(null))
        assertNull(folderLabel("content://com.android.externalstorage.documents/document/primary%3AMusic"))
    }
}
