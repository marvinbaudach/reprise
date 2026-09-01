package io.github.marvinbaudach.reprise

import android.os.Looper

internal fun requireOffMainThread(what: String) {
    if (!BuildConfig.DEBUG) return
    val main = Looper.getMainLooper() ?: return
    check(Looper.myLooper() !== main) {
        "$what writes on the main thread; queue it on LibraryWrites"
    }
}
