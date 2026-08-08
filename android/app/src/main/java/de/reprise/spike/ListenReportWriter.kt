package de.reprise.spike

import android.content.ContentResolver
import android.net.Uri
import android.os.Bundle
import android.provider.DocumentsContract
import java.io.FileNotFoundException
import java.io.IOException

internal const val LISTEN_REPORT_FILE_NAME = "reprise-listens-back.rpl"
internal const val LISTEN_REPORT_ACKNOWLEDGEMENT_FILE_NAME = "reprise-listens-back-ack.rpl"

/**
 * Moves Core-produced report bytes across the Android storage boundary.
 *
 * Reading the acknowledgement is deliberately fail-open: absence, truncation,
 * revoked access and provider I/O all become `null`, which tells Rust to prune
 * nothing. Report production or writing remains an explicit failure.
 */
internal class ListenReportWriter(
    private val readAcknowledgement: () -> ByteArray?,
    private val produceReport: (ByteArray?) -> ByteArray,
    private val writeReport: (ByteArray) -> Unit,
) {
    fun publish(): Result<Unit> {
        val acknowledgement = runCatching(readAcknowledgement).getOrNull()
        return runCatching { writeReport(produceReport(acknowledgement)) }
    }
}

/** Kotlin's read/write ownership of the selected DocumentsProvider tree. */
internal class AndroidListenReportFiles(
    private val resolver: ContentResolver,
    private val treeUri: Uri,
) {
    fun readAcknowledgement(): ByteArray? {
        val document = child(LISTEN_REPORT_ACKNOWLEDGEMENT_FILE_NAME) ?: return null
        return resolver.openInputStream(document)?.use { input -> input.readBytes() }
            ?: throw IOException("The provider returned no acknowledgement stream")
    }

    fun writeReport(bytes: ByteArray) {
        synchronized(REPORT_WRITE_LOCK) {
            // Library configuration and the playback service can publish at
            // the same time. Keep lookup, optional creation and truncation one
            // transaction so both cannot observe the canonical name missing.
            val document = child(LISTEN_REPORT_FILE_NAME) ?: createReport()
            resolver.openOutputStream(document, "wt")?.use { output ->
                output.write(bytes)
                output.flush()
            } ?: throw IOException("The provider returned no report stream")
        }
    }

    private fun child(displayName: String): Uri? {
        val rootId = DocumentsContract.getTreeDocumentId(treeUri)
        val children = DocumentsContract.buildChildDocumentsUriUsingTree(treeUri, rootId)
        return resolver.query(children, CHILD_PROJECTION, Bundle.EMPTY, null)?.use { cursor ->
            val idColumn = cursor.getColumnIndexOrThrow(
                DocumentsContract.Document.COLUMN_DOCUMENT_ID,
            )
            val nameColumn = cursor.getColumnIndexOrThrow(
                DocumentsContract.Document.COLUMN_DISPLAY_NAME,
            )
            while (cursor.moveToNext()) {
                if (cursor.getString(nameColumn) == displayName) {
                    return@use DocumentsContract.buildDocumentUriUsingTree(
                        treeUri,
                        cursor.getString(idColumn),
                    )
                }
            }
            null
        }
    }

    private fun createReport(): Uri {
        val root = DocumentsContract.buildDocumentUriUsingTree(
            treeUri,
            DocumentsContract.getTreeDocumentId(treeUri),
        )
        return DocumentsContract.createDocument(
            resolver,
            root,
            "application/octet-stream",
            LISTEN_REPORT_FILE_NAME,
        ) ?: throw FileNotFoundException("The provider did not create the listen report")
    }

    private companion object {
        val REPORT_WRITE_LOCK = Any()
        val CHILD_PROJECTION = arrayOf(
            DocumentsContract.Document.COLUMN_DOCUMENT_ID,
            DocumentsContract.Document.COLUMN_DISPLAY_NAME,
        )
    }
}
