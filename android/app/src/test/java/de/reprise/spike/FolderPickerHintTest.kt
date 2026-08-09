package de.reprise.spike

import android.net.Uri
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class FolderPickerHintTest {
    @Test
    fun existingRepriseFolderIsThePickerHint() {
        assertEquals(
            Uri.parse(
                "content://com.android.externalstorage.documents/" +
                    "tree/primary%3AMusic%2FReprise",
            ),
            folderPickerInitialUri { true },
        )
    }

    @Test
    fun missingOrUninspectableRepriseFolderFallsBackToMusic() {
        val music = Uri.parse(
            "content://com.android.externalstorage.documents/tree/primary%3AMusic",
        )

        assertEquals(music, folderPickerInitialUri { false })
        assertEquals(music, folderPickerInitialUri { error("storage is unavailable") })
    }
}
