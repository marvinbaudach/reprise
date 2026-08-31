package io.github.marvinbaudach.reprise

import android.Manifest
import android.content.Context
import android.content.pm.ProviderInfo
import android.database.Cursor
import android.database.MatrixCursor
import android.net.Uri
import android.os.CancellationSignal
import android.os.ParcelFileDescriptor
import android.provider.DocumentsContract
import android.provider.DocumentsProvider
import java.io.File
import java.io.IOException
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.shadows.ShadowContentResolver

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class ListenReportWriterTest {
    @Test
    fun missingAcknowledgementMeansNothingWasAcknowledgedAndTheReportIsStillWritten() {
        var receivedAcknowledgement: ByteArray? = byteArrayOf(99)
        var written: ByteArray? = null
        val writer = ListenReportWriter(
            readAcknowledgement = { null },
            produceReport = { acknowledgement ->
                receivedAcknowledgement = acknowledgement
                byteArrayOf(1, 2, 3)
            },
            writeReport = { bytes -> written = bytes },
        )

        assertTrue(writer.publish().isSuccess)
        assertNull(receivedAcknowledgement)
        assertArrayEquals(byteArrayOf(1, 2, 3), written)
    }

    @Test
    fun unreadableAcknowledgementAlsoMeansNothingWasAcknowledged() {
        var receivedAcknowledgement: ByteArray? = byteArrayOf(99)
        var writes = 0
        val writer = ListenReportWriter(
            readAcknowledgement = { throw IOException("truncated provider read") },
            produceReport = { acknowledgement ->
                receivedAcknowledgement = acknowledgement
                byteArrayOf(4, 5, 6)
            },
            writeReport = { writes++ },
        )

        assertTrue(writer.publish().isSuccess)
        assertNull(receivedAcknowledgement)
        assertEquals(1, writes)
    }

    @Test
    fun concurrentWritesReplaceTheCanonicalDocumentAndIgnoreNumberedAcknowledgements() {
        val provider = RacingDocumentsProvider.install()
        provider.addDocument(
            LISTEN_REPORT_ACKNOWLEDGEMENT_FILE_NAME,
            byteArrayOf(7, 8, 9),
        )
        provider.addDocument(
            "reprise-listens-back-ack (1).rpl",
            byteArrayOf(99),
        )
        val resolver = RuntimeEnvironment.getApplication().contentResolver
        val first = AndroidListenReportFiles(resolver, provider.treeUri)
        val second = AndroidListenReportFiles(resolver, provider.treeUri)
        val executor = Executors.newFixedThreadPool(2)

        try {
            val firstWrite = executor.submit { first.writeReport(byteArrayOf(1, 2, 3)) }
            if (!provider.firstEmptyQuery.await(5, TimeUnit.SECONDS)) firstWrite.get()
            val secondWrite = executor.submit { second.writeReport(byteArrayOf(4, 5)) }
            firstWrite.get(5, TimeUnit.SECONDS)
            secondWrite.get(5, TimeUnit.SECONDS)
        } finally {
            executor.shutdownNow()
        }

        assertEquals(listOf(LISTEN_REPORT_FILE_NAME), provider.reportNames())
        assertArrayEquals(byteArrayOf(4, 5), provider.bytes(LISTEN_REPORT_FILE_NAME))
        assertArrayEquals(
            byteArrayOf(7, 8, 9),
            AndroidListenReportFiles(resolver, provider.treeUri).readAcknowledgement(),
        )
    }
}

private class RacingDocumentsProvider : DocumentsProvider() {
    private data class Document(
        val id: String,
        val displayName: String,
        val file: File,
    )

    val treeUri: Uri = Uri.parse("content://$AUTHORITY/tree/$ROOT_ID")
    val firstEmptyQuery = CountDownLatch(1)

    private val bothEmptyQueries = CountDownLatch(2)
    private val documents = linkedMapOf<String, Document>()
    private lateinit var storage: File
    private var nextId = 1

    override fun onCreate(): Boolean {
        storage = checkNotNull(context).cacheDir.resolve("listen-report-provider").also {
            it.mkdirs()
        }
        return true
    }

    override fun queryRoots(projection: Array<out String>?): Cursor =
        MatrixCursor(projection ?: emptyArray())

    override fun queryDocument(documentId: String, projection: Array<out String>?): Cursor =
        documentCursor(projection, listOfNotNull(documents[documentId]))

    override fun queryChildDocuments(
        parentDocumentId: String,
        projection: Array<out String>?,
        sortOrder: String?,
    ): Cursor {
        check(parentDocumentId == ROOT_ID)
        val snapshot = synchronized(this) { documents.values.toList() }
        if (snapshot.none { it.displayName == LISTEN_REPORT_FILE_NAME }) {
            firstEmptyQuery.countDown()
            bothEmptyQueries.countDown()
            bothEmptyQueries.await(1, TimeUnit.SECONDS)
        }
        return documentCursor(projection, snapshot)
    }

    override fun isChildDocument(parentDocumentId: String, documentId: String): Boolean =
        parentDocumentId == ROOT_ID && synchronized(this) { documentId in documents }

    @Synchronized
    override fun createDocument(
        parentDocumentId: String,
        mimeType: String,
        displayName: String,
    ): String {
        check(parentDocumentId == ROOT_ID)
        val actualName = uniqueName(displayName)
        return addDocument(actualName, byteArrayOf())
    }

    override fun openDocument(
        documentId: String,
        mode: String,
        signal: CancellationSignal?,
    ): ParcelFileDescriptor {
        val document = synchronized(this) { checkNotNull(documents[documentId]) }
        return ParcelFileDescriptor.open(document.file, ParcelFileDescriptor.parseMode(mode))
    }

    @Synchronized
    fun addDocument(displayName: String, bytes: ByteArray): String {
        val id = "document-${nextId++}"
        val file = storage.resolve(id)
        file.writeBytes(bytes)
        documents[id] = Document(id, displayName, file)
        return id
    }

    @Synchronized
    fun reportNames(): List<String> = documents.values
        .map(Document::displayName)
        .filter { it.startsWith("reprise-listens-back") && !it.contains("-ack") }
        .sorted()

    @Synchronized
    fun bytes(displayName: String): ByteArray = documents.values
        .single { it.displayName == displayName }
        .file
        .readBytes()

    private fun documentCursor(
        projection: Array<out String>?,
        rows: List<Document>,
    ): Cursor {
        val columns = projection ?: DOCUMENT_PROJECTION
        return MatrixCursor(columns).also { cursor ->
            for (document in rows) {
                cursor.newRow().also { row ->
                    for (column in columns) {
                        row.add(
                            when (column) {
                                DocumentsContract.Document.COLUMN_DOCUMENT_ID -> document.id
                                DocumentsContract.Document.COLUMN_DISPLAY_NAME -> document.displayName
                                DocumentsContract.Document.COLUMN_MIME_TYPE -> "application/octet-stream"
                                else -> null
                            },
                        )
                    }
                }
            }
        }
    }

    private fun uniqueName(displayName: String): String {
        val names = documents.values.map(Document::displayName).toSet()
        if (displayName !in names) return displayName
        val dot = displayName.lastIndexOf('.')
        val stem = if (dot >= 0) displayName.substring(0, dot) else displayName
        val suffix = if (dot >= 0) displayName.substring(dot) else ""
        return generateSequence(1) { it + 1 }
            .map { "$stem ($it)$suffix" }
            .first { it !in names }
    }

    companion object {
        private const val AUTHORITY = "io.github.marvinbaudach.reprise.listen-report-test"
        private const val ROOT_ID = "root"
        private val DOCUMENT_PROJECTION = arrayOf(
            DocumentsContract.Document.COLUMN_DOCUMENT_ID,
            DocumentsContract.Document.COLUMN_DISPLAY_NAME,
            DocumentsContract.Document.COLUMN_MIME_TYPE,
        )

        fun install(): RacingDocumentsProvider {
            val context = RuntimeEnvironment.getApplication() as Context
            return RacingDocumentsProvider().also { provider ->
                provider.attachInfo(
                    context,
                    ProviderInfo().apply {
                        authority = AUTHORITY
                        exported = true
                        grantUriPermissions = true
                        readPermission = Manifest.permission.MANAGE_DOCUMENTS
                        writePermission = Manifest.permission.MANAGE_DOCUMENTS
                    },
                )
                ShadowContentResolver.registerProviderInternal(AUTHORITY, provider)
            }
        }
    }
}
