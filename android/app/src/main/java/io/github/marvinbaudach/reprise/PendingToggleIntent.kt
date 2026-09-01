package io.github.marvinbaudach.reprise

/** Alternates rapid toggle intentions without changing the displayed answer. */
internal class PendingToggleIntent {
    private var lastSubmitted: Boolean? = null

    fun next(displayed: Boolean): Boolean {
        val target = !(lastSubmitted ?: displayed)
        lastSubmitted = target
        return target
    }

    fun answered(target: Boolean) {
        if (lastSubmitted == target) lastSubmitted = null
    }
}
