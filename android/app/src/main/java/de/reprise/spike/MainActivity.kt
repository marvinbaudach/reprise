package de.reprise.spike

import android.os.Bundle
import android.util.Log
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import java.io.File
import kotlin.system.measureTimeMillis
import uniffi.reprise_android_ffi.MusicLibrary
import uniffi.reprise_android_ffi.TrackRow

private const val TAG = "RepriseSpike"

/**
 * The whole point of this activity: prove that `reprise-core` loads as a
 * `.so` inside a real app sandbox — different SELinux context, different
 * paths and a different process model than the `adb shell` run that Stage 1
 * used — and that it can create its database and scan audio files in
 * app-private storage without holding a single Android permission.
 */
class MainActivity : ComponentActivity() {

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val report = runProbe()
        setContent {
            MaterialTheme {
                Surface(modifier = Modifier.fillMaxSize()) {
                    ProbeScreen(report)
                }
            }
        }
    }

    private fun runProbe(): ProbeReport = try {
        val musicDir = File(filesDir, "music").apply { mkdirs() }
        copyAsset("sine.flac", File(musicDir, "01-sine.flac"))
        copyAsset("sine.flac", File(musicDir, "02-sine-copy.flac"))
        copyAsset("broken-tags.mp3", File(musicDir, "03-broken.mp3"))

        val dbFile = File(filesDir, "reprise.db")
        lateinit var library: MusicLibrary
        val openMs = measureTimeMillis { library = MusicLibrary.open(dbFile.absolutePath) }

        var added = 0u
        var errors = 0u
        val scanMs = measureTimeMillis {
            val summary = library.scan(musicDir.absolutePath)
            added = summary.added
            errors = summary.errors
        }

        var count = 0L
        val countMs = measureTimeMillis { count = library.trackCount() }

        var rows: List<TrackRow> = emptyList()
        val windowMs = measureTimeMillis { rows = library.window(0L, 50L) }

        ProbeReport(
            storagePath = filesDir.absolutePath,
            openMs = openMs,
            scanMs = scanMs,
            countMs = countMs,
            windowMs = windowMs,
            added = added.toInt(),
            errors = errors.toInt(),
            trackCount = count,
            rows = rows,
        ).also { Log.i(TAG, "PROBE OK $it") }
    } catch (error: Throwable) {
        Log.e(TAG, "PROBE FAILED", error)
        ProbeReport(failure = "${error::class.java.simpleName}: ${error.message}")
    }

    private fun copyAsset(assetName: String, target: File) {
        assets.open(assetName).use { input ->
            target.outputStream().use { output -> input.copyTo(output) }
        }
    }
}

data class ProbeReport(
    val storagePath: String = "",
    val openMs: Long = 0,
    val scanMs: Long = 0,
    val countMs: Long = 0,
    val windowMs: Long = 0,
    val added: Int = 0,
    val errors: Int = 0,
    val trackCount: Long = 0,
    val rows: List<TrackRow> = emptyList(),
    val failure: String? = null,
)

@Composable
private fun ProbeScreen(report: ProbeReport) {
    if (report.failure != null) {
        Text(
            text = "FAILED\n${report.failure}",
            style = MaterialTheme.typography.bodyLarge,
            modifier = Modifier.padding(24.dp),
        )
        return
    }

    LazyColumn(contentPadding = PaddingValues(20.dp)) {
        item {
            Column {
                Text("reprise-core in an app sandbox", style = MaterialTheme.typography.headlineSmall)
                Text(report.storagePath, style = MaterialTheme.typography.bodySmall)
                Text(
                    "open+migrate ${report.openMs} ms · scan ${report.scanMs} ms " +
                        "· count ${report.countMs} ms · window ${report.windowMs} ms",
                    style = MaterialTheme.typography.bodyMedium,
                    modifier = Modifier.padding(top = 12.dp),
                )
                Text(
                    "scanned: added=${report.added} errors=${report.errors} · " +
                        "library holds ${report.trackCount} tracks",
                    style = MaterialTheme.typography.bodyMedium,
                )
                HorizontalDivider(modifier = Modifier.padding(vertical = 16.dp))
            }
        }
        items(report.rows) { row ->
            Column(modifier = Modifier.padding(vertical = 6.dp)) {
                Text(row.title, style = MaterialTheme.typography.titleMedium)
                Text(
                    "${row.artist} — ${row.album} · ${row.durationMs} ms",
                    style = MaterialTheme.typography.bodySmall,
                )
            }
        }
    }
}
