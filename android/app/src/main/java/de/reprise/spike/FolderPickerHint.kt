package de.reprise.spike

import android.net.Uri
import android.os.Environment
import android.provider.DocumentsContract
import java.io.File

private const val EXTERNAL_STORAGE_DOCUMENTS = "com.android.externalstorage.documents"
private const val PRIMARY_VOLUME = "primary"
private const val MUSIC_DOCUMENT = "$PRIMARY_VOLUME:Music"
private const val REPRISE_MUSIC_DOCUMENT = "$MUSIC_DOCUMENT/Reprise"

/**
 * Points Android's user-controlled tree picker at Reprise's managed music
 * folder when the ordinary filesystem view can confirm that it exists. A
 * scoped-storage refusal is the same as absence here: Music is a useful hint,
 * and Android remains free to ignore either hint and use its normal location.
 */
internal fun folderPickerInitialUri(
    repriseFolderExists: () -> Boolean = ::repriseMusicFolderExists,
): Uri {
    val documentId = if (runCatching(repriseFolderExists).getOrDefault(false)) {
        REPRISE_MUSIC_DOCUMENT
    } else {
        MUSIC_DOCUMENT
    }
    return DocumentsContract.buildTreeDocumentUri(EXTERNAL_STORAGE_DOCUMENTS, documentId)
}

@Suppress("DEPRECATION") // The shared Music directory is the picker hint, not app-owned storage.
private fun repriseMusicFolderExists(): Boolean = File(
    Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_MUSIC),
    "Reprise",
).isDirectory
