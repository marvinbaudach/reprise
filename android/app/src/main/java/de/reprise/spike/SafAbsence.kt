package de.reprise.spike

import java.io.FileNotFoundException
import java.util.Collections
import java.util.IdentityHashMap

private const val MAX_CAUSE_DEPTH = 64

/**
 * Whether this failure is the provider stating that the document does not
 * exist, as opposed to a failure to reach it.
 *
 * Android's DocumentsProvider.enforceTree() calls isChildDocument(), and
 * ExternalStorageProvider raises a RuntimeException whose cause is the
 * FileNotFoundException -- measured 2026-08-22 on a Pixel 10 Pro XL. So the
 * whole cause chain is inspected and the message text never is: that string
 * belongs to one provider on one Android version, and matching it is how this
 * bug comes back.
 */
internal fun Throwable.confirmsAbsence(): Boolean {
    val visited = Collections.newSetFromMap(IdentityHashMap<Throwable, Boolean>())
    var current: Throwable? = this

    repeat(MAX_CAUSE_DEPTH) {
        val error = current ?: return false
        if (!visited.add(error)) return false
        if (error is FileNotFoundException) return true
        current = error.cause
    }
    return false
}
