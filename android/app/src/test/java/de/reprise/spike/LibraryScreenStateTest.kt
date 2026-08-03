package de.reprise.spike

fun main() {
    unknownTotalUsesIndeterminateProgress()
    knownTotalUsesHonestProgressFraction()
}

private fun unknownTotalUsesIndeterminateProgress() {
    val scanning = LibraryScreenState.Scanning(processed = 1u, total = null)

    check(scanning.progressPresentation() == ScanProgressPresentation.Indeterminate)
}

private fun knownTotalUsesHonestProgressFraction() {
    val scanning = LibraryScreenState.Scanning(processed = 1u, total = 4u)

    check(scanning.progressPresentation() == ScanProgressPresentation.Determinate(0.25f))
}
