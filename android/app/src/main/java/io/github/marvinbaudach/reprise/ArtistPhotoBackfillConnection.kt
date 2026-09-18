package io.github.marvinbaudach.reprise

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

/**
 * One progress bar for both passes (decision 9, "same handle, same
 * progress"): the cover pass that rides after the portraits (B3) has no
 * field of its own on [ArtistPhotoProgress] — its counts are folded into
 * `done`/`total` instead. Before the cover pass starts, `coversDone` and
 * `coversTotal` are both `0`, so this is exactly the portrait-only progress
 * the bar already showed.
 */
private fun ArtistPortraitProgressUpdate.toUiProgress() = ArtistPhotoProgress(
    runId = runId.toLong(),
    phase = when (state) {
        ArtistPortraitProgressState.PREPARING -> ArtistPhotoProgressPhase.PREPARING
        ArtistPortraitProgressState.RUNNING -> ArtistPhotoProgressPhase.RUNNING
        ArtistPortraitProgressState.PAUSED -> ArtistPhotoProgressPhase.PAUSED
        ArtistPortraitProgressState.COMPLETE -> ArtistPhotoProgressPhase.COMPLETE
    },
    done = (done + coversDone).toLong(),
    failed = failed.toLong(),
    total = (total + coversTotal).toLong(),
)
