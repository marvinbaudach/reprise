package de.reprise.spike

import uniffi.reprise_android_ffi.ArtistPortraitProgressListener
import uniffi.reprise_android_ffi.ArtistPortraitProgressState
import uniffi.reprise_android_ffi.ArtistPortraitProgressUpdate
import uniffi.reprise_android_ffi.MusicLibrary

/** Rebinds an activity callback while the ViewModel and Rust worker stay alive. */
internal fun MobileSurfaceViewModel.connectArtistPhotoBackfill(
    library: MusicLibrary,
    postToMain: (() -> Unit) -> Unit,
) {
    bindArtistPhotoBackfill(
        snapshot = { library.artistPortraitBackfillProgress().toUiProgress() },
        start = { deliver ->
            library.startArtistPortraitBackfill(
                object : ArtistPortraitProgressListener {
                    override fun onProgress(update: ArtistPortraitProgressUpdate) {
                        deliver(update.toUiProgress())
                    }
                },
            )
        },
        cancel = library::cancelArtistPortraitBackfill,
        postToMain = postToMain,
    )
}

private fun ArtistPortraitProgressUpdate.toUiProgress() = ArtistPhotoProgress(
    runId = runId.toLong(),
    phase = when (state) {
        ArtistPortraitProgressState.PREPARING -> ArtistPhotoProgressPhase.PREPARING
        ArtistPortraitProgressState.RUNNING -> ArtistPhotoProgressPhase.RUNNING
        ArtistPortraitProgressState.PAUSED -> ArtistPhotoProgressPhase.PAUSED
        ArtistPortraitProgressState.COMPLETE -> ArtistPhotoProgressPhase.COMPLETE
    },
    done = done.toLong(),
    failed = failed.toLong(),
    total = total.toLong(),
)
