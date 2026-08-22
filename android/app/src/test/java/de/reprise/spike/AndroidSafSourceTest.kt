package de.reprise.spike

import android.content.ContentProvider
import android.content.ContentValues
import android.content.Context
import android.content.pm.ProviderInfo
import android.database.Cursor
import android.database.MatrixCursor
import android.net.Uri
import android.provider.DocumentsContract
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.shadows.ShadowContentResolver
import uniffi.reprise_android_ffi.SafSourceException

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class AndroidSafSourceTest {
    @Test
    fun emptyMetadataCursorMeansTheDocumentIsAbsent() {
        val fixture = MetadataProvider.install("empty") {
            MatrixCursor(METADATA_PROJECTION)
        }

        assertNull(fixture.source.probe(fixture.treeUri.toString(), true))
    }

    @Test
    fun nullMetadataCursorRemainsUnknown() {
        val fixture = MetadataProvider.install("null") { null }

        assertThrows(SafSourceException.Unknown::class.java) {
            fixture.source.probe(fixture.treeUri.toString(), true)
        }
    }

    private companion object {
        val METADATA_PROJECTION = arrayOf(
            DocumentsContract.Document.COLUMN_DOCUMENT_ID,
            DocumentsContract.Document.COLUMN_DISPLAY_NAME,
            DocumentsContract.Document.COLUMN_MIME_TYPE,
            DocumentsContract.Document.COLUMN_SIZE,
            DocumentsContract.Document.COLUMN_LAST_MODIFIED,
        )
    }
}

private class MetadataProvider(
    private val queryResult: () -> Cursor?,
) : ContentProvider() {
    override fun onCreate(): Boolean = true

    override fun query(
        uri: Uri,
        projection: Array<out String>?,
        selection: String?,
        selectionArgs: Array<out String>?,
        sortOrder: String?,
    ): Cursor? = queryResult()

    override fun getType(uri: Uri): String? = null

    override fun insert(uri: Uri, values: ContentValues?): Uri? = null

    override fun delete(uri: Uri, selection: String?, selectionArgs: Array<out String>?): Int = 0

    override fun update(
        uri: Uri,
        values: ContentValues?,
        selection: String?,
        selectionArgs: Array<out String>?,
    ): Int = 0

    companion object {
        fun install(name: String, queryResult: () -> Cursor?): MetadataFixture {
            val authority = "de.reprise.spike.saf-absence-$name"
            val context = RuntimeEnvironment.getApplication() as Context
            val provider = MetadataProvider(queryResult)
            provider.attachInfo(
                context,
                ProviderInfo().apply {
                    this.authority = authority
                    exported = true
                    grantUriPermissions = true
                },
            )
            ShadowContentResolver.registerProviderInternal(authority, provider)
            val treeUri = Uri.parse("content://$authority/tree/root")
            return MetadataFixture(
                treeUri,
                AndroidSafSource(context.contentResolver, treeUri),
            )
        }
    }
}

private data class MetadataFixture(
    val treeUri: Uri,
    val source: AndroidSafSource,
)
