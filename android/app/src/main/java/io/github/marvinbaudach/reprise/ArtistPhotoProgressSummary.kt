package io.github.marvinbaudach.reprise

internal fun artistPhotoProgressSummarySuffix(progress: ArtistPhotoProgress?): String =
    when (progress?.phase) {
        null -> ""
        ArtistPhotoProgressPhase.PREPARING -> " · Preparing artwork"
        ArtistPhotoProgressPhase.RUNNING -> " · Artwork ${progress.done}/${progress.total}"
        ArtistPhotoProgressPhase.PAUSED -> " · Waiting for a connection"
        ArtistPhotoProgressPhase.COMPLETE -> {
            if (progress.failed > 0L) " · ${progress.failed} without a photo" else ""
        }
    }
