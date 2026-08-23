package de.reprise.spike

import android.content.ContentResolver
import android.database.Cursor
import android.net.Uri
import android.os.ParcelFileDescriptor
import android.provider.DocumentsContract
import java.io.IOException
import uniffi.reprise_android_ffi.SafSource
import uniffi.reprise_android_ffi.SafSourceException
import uniffi.reprise_android_ffi.SourceChild
import uniffi.reprise_android_ffi.SourceFacts

internal class AndroidSafSource(
    private val resolver: ContentResolver,
    private val treeUri: Uri,
    private val detachFd: (ParcelFileDescriptor) -> Int = { it.detachFd() },
) : SafSource {
    private val treeToken = stableTreeToken(treeUri)

    override fun residenceToken(uri: String): Long = treeToken

    override fun probe(uri: String, followLinks: Boolean): SourceFacts? {
        val documentUri = asDocumentUri(Uri.parse(uri))
        return try {
            val cursor = resolver.query(documentUri, METADATA_PROJECTION, null, null, null)
                ?: throw SafSourceException.Unknown("The provider returned no metadata cursor")
            cursor.use {
                if (!it.moveToFirst()) return null
                it.sourceFacts()
            }
        } catch (error: SecurityException) {
            throw SafSourceException.PermissionDenied(error.detail())
        } catch (error: SafSourceException) {
            throw error
        } catch (error: IOException) {
            if (error.confirmsAbsence()) null else throw SafSourceException.Io(error.detail())
        } catch (error: RuntimeException) {
            if (error.confirmsAbsence()) null else throw SafSourceException.Unknown(error.detail())
        }
    }

    override fun listChildren(uri: String): List<SourceChild> {
        val parentUri = Uri.parse(uri)
        val parentId = if (parentUri.pathSegments.contains("document")) {
            DocumentsContract.getDocumentId(parentUri)
        } else {
            DocumentsContract.getTreeDocumentId(parentUri)
        }
        val childrenUri = DocumentsContract.buildChildDocumentsUriUsingTree(treeUri, parentId)

        // Absence classification deliberately does not apply here: turning a failed listing
        // into an empty directory could mass-mark a whole subtree missing.
        return try {
            resolver.query(childrenUri, CHILD_PROJECTION, null, null, null)?.use { cursor ->
                buildList {
                    while (cursor.moveToNext()) {
                        add(cursor.sourceChild())
                    }
                }
            } ?: throw SafSourceException.Unknown("The provider returned no child cursor")
        } catch (error: SecurityException) {
            throw SafSourceException.PermissionDenied(error.detail())
        } catch (error: IOException) {
            throw SafSourceException.Io(error.detail())
        } catch (error: SafSourceException) {
            throw error
        } catch (error: RuntimeException) {
            throw SafSourceException.Unknown(error.detail())
        }
    }

    override fun openReadFd(uri: String): Int = try {
        resolver.openFileDescriptor(Uri.parse(uri), "r")?.let(detachFd)
            ?: throw SafSourceException.Io("The provider returned no file descriptor")
    } catch (error: SecurityException) {
        throw SafSourceException.PermissionDenied(error.detail())
    } catch (error: SafSourceException) {
        throw error
    } catch (error: IOException) {
        if (error.confirmsAbsence()) {
            throw SafSourceException.NotFound(error.detail())
        }
        throw SafSourceException.Io(error.detail())
    } catch (error: RuntimeException) {
        if (error.confirmsAbsence()) {
            throw SafSourceException.NotFound(error.detail())
        }
        throw SafSourceException.Unknown(error.detail())
    }

    private fun asDocumentUri(uri: Uri): Uri =
        if (uri.pathSegments.contains("document")) {
            uri
        } else {
            DocumentsContract.buildDocumentUriUsingTree(
                treeUri,
                DocumentsContract.getTreeDocumentId(uri),
            )
        }

    private fun Cursor.sourceFacts(): SourceFacts {
        val mimeType = requiredString(DocumentsContract.Document.COLUMN_MIME_TYPE)
        return SourceFacts(
            displayName = optionalString(DocumentsContract.Document.COLUMN_DISPLAY_NAME),
            isFile = mimeType != DocumentsContract.Document.MIME_TYPE_DIR,
            isDirectory = mimeType == DocumentsContract.Document.MIME_TYPE_DIR,
            sizeBytes = optionalLong(DocumentsContract.Document.COLUMN_SIZE)?.toULong(),
            modifiedUnixMs = optionalLong(DocumentsContract.Document.COLUMN_LAST_MODIFIED),
            documentId = requiredString(DocumentsContract.Document.COLUMN_DOCUMENT_ID),
        )
    }

    private fun Cursor.sourceChild(): SourceChild {
        val documentId = requiredString(DocumentsContract.Document.COLUMN_DOCUMENT_ID)
        val mimeType = requiredString(DocumentsContract.Document.COLUMN_MIME_TYPE)
        return SourceChild(
            uri = DocumentsContract.buildDocumentUriUsingTree(treeUri, documentId).toString(),
            displayName = optionalString(DocumentsContract.Document.COLUMN_DISPLAY_NAME),
            isFile = mimeType != DocumentsContract.Document.MIME_TYPE_DIR,
            isDirectory = mimeType == DocumentsContract.Document.MIME_TYPE_DIR,
            sizeBytes = optionalLong(DocumentsContract.Document.COLUMN_SIZE)?.toULong(),
            modifiedUnixMs = optionalLong(DocumentsContract.Document.COLUMN_LAST_MODIFIED),
            documentId = documentId,
        )
    }

    private fun Cursor.requiredString(column: String): String =
        getString(getColumnIndexOrThrow(column))

    private fun Cursor.optionalString(column: String): String? {
        val index = getColumnIndexOrThrow(column)
        return if (isNull(index)) null else getString(index)
    }

    private fun Cursor.optionalLong(column: String): Long? {
        val index = getColumnIndexOrThrow(column)
        return if (isNull(index)) null else getLong(index)
    }

    private fun Throwable.detail(): String = message ?: javaClass.simpleName

    private fun stableTreeToken(uri: Uri): Long {
        val authority = uri.authority.orEmpty().hashCode().toLong()
        val documentId = DocumentsContract.getTreeDocumentId(uri).hashCode().toLong()
        return (authority shl 32) xor (documentId and 0xffff_ffffL)
    }

    private companion object {
        val METADATA_PROJECTION = arrayOf(
            DocumentsContract.Document.COLUMN_DOCUMENT_ID,
            DocumentsContract.Document.COLUMN_DISPLAY_NAME,
            DocumentsContract.Document.COLUMN_MIME_TYPE,
            DocumentsContract.Document.COLUMN_SIZE,
            DocumentsContract.Document.COLUMN_LAST_MODIFIED,
        )
        val CHILD_PROJECTION = arrayOf(
            DocumentsContract.Document.COLUMN_DOCUMENT_ID,
            DocumentsContract.Document.COLUMN_DISPLAY_NAME,
            DocumentsContract.Document.COLUMN_MIME_TYPE,
            DocumentsContract.Document.COLUMN_SIZE,
            DocumentsContract.Document.COLUMN_LAST_MODIFIED,
        )
    }
}
