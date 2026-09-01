package io.github.marvinbaudach.reprise

import android.app.Application
import android.util.Log
import uniffi.reprise_android_ffi.initLogging

/**
 * The one place the native library's diagnostics are switched on.
 *
 * `tracing` discards every event while no subscriber is installed, so until
 * this call the library's warnings — an unwritable cover cache, a play count
 * that never reached the database — existed only in Rust tests. `onCreate` runs
 * once per process and before both entry points into the library: the activity
 * that opens `MusicLibrary` and the playback service Media3 may start on its
 * own.
 *
 * Repeating the call is harmless by construction (see the Rust side's
 * `logging` module), which is what lets this stay a plain statement rather than
 * a guarded one.
 *
 * Read the result with `adb logcat -s Reprise`.
 */
class RepriseApplication : Application() {
    override fun onCreate() {
        super.onCreate()
        // The call itself cannot fail; loading the `.so` behind it can, and
        // this is now the first thing in the process that touches it. Today
        // that failure surfaces on the library screen as an ordinary error —
        // turning on the logging must not turn it into a startup crash.
        runCatching { initLogging() }.onFailure { error ->
            Log.e("Reprise", "Could not install native logging", error)
        }
    }
}
