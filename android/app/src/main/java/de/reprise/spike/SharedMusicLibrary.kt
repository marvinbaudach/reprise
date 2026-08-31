package de.reprise.spike

import android.app.Application
import android.content.Context
import java.util.WeakHashMap
import uniffi.reprise_android_ffi.MusicLibrary

private val processLibraries = WeakHashMap<Application, MusicLibrary>()

/** Returns the one native library handle owned by this application process. */
internal fun Context.sharedMusicLibrary(): MusicLibrary {
    val app = applicationContext as Application
    return synchronized(processLibraries) {
        processLibraries.getOrPut(app) {
            MusicLibrary.open(app.filesDir.absolutePath, app.cacheDir.absolutePath)
        }
    }
}
